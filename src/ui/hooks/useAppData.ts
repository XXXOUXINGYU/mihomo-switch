import { useCallback, useEffect, useRef, useState } from "react";

import {
  bindPortSlot,
  bindPortSlotsBatch,
  bootstrap,
  clearPortSlotBinding,
  createPortSlot,
  createSubscription,
  deletePortSlot,
  deleteSubscription,
  getPortTraffic,
  importSubscription,
  listenLogs,
  reorderPortSlots,
  saveProxySettings,
  setPortSlotEnabled,
  startMihomo,
  stopMihomo,
  cancelLatency,
  testLatency,
  updatePortSlot,
  updateSubscription,
  validatePort,
} from "../tauri";
import type {
  AppSettings,
  BootstrapPayload,
  LogEntry,
  PortSlotBindingInput,
  PortSlotInput,
  PortTrafficReport,
  PortValidation,
  SubscriptionRecord,
  UpsertSubscriptionInput,
} from "../types";
import { buildSlotViews } from "../lib/slots";
import { buildPreviewTraffic, createPreviewPayload } from "../lib/preview";
import { useToast } from "./useToast";

function isTauriRuntime() {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function makeId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `local-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function timestamp() {
  return new Date().toLocaleTimeString("zh-CN", { hour12: false });
}

export type LatencyMap = Record<string, string>;

export function latencyKey(subId: string, nodeIndex: number) {
  return `${subId}:${nodeIndex}`;
}

export type LatencyTestTarget = {
  subId: string;
  nodeIndex: number;
};

export type LatencyBatchState = {
  running: boolean;
  done: number;
  total: number;
};

const INITIAL_LATENCY_BATCH: LatencyBatchState = {
  running: false,
  done: 0,
  total: 0,
};

function dedupeLatencyTargets(targets: LatencyTestTarget[]) {
  const seen = new Set<string>();
  const unique: LatencyTestTarget[] = [];
  for (const target of targets) {
    const key = latencyKey(target.subId, target.nodeIndex);
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    unique.push(target);
  }
  return unique;
}

function groupTargetsBySubscription(targets: LatencyTestTarget[]) {
  const groups = new Map<string, number[]>();
  for (const target of targets) {
    const indices = groups.get(target.subId) ?? [];
    indices.push(target.nodeIndex);
    groups.set(target.subId, indices);
  }
  return groups;
}

export function useAppData() {
  const browserPreview = !isTauriRuntime();
  const [payload, setPayload] = useState<BootstrapPayload | null>(null);
  const [bootstrapError, setBootstrapError] = useState<string | null>(null);
  const [bootstrapping, setBootstrapping] = useState(true);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [latencyMap, setLatencyMap] = useState<LatencyMap>({});
  const [latencyBatch, setLatencyBatch] = useState<LatencyBatchState>(INITIAL_LATENCY_BATCH);
  const [portTraffic, setPortTraffic] = useState<PortTrafficReport | null>(null);
  const [portTrafficError, setPortTrafficError] = useState<string | null>(null);
  const payloadRef = useRef<BootstrapPayload | null>(null);
  const mutationInFlight = useRef(false);
  const latencyQueue = useRef<Promise<void>>(Promise.resolve());
  const pendingLatencyKeys = useRef(new Set<string>());
  const latencyBatchRunning = useRef(false);
  const latencyBatchCancel = useRef(false);
  const { toasts, push: pushToast, dismiss: dismissToast } = useToast();

  const appendLog = useCallback((level: string, message: string) => {
    setLogs((prev) => [...prev.slice(-299), { level, message, timestamp: timestamp() }]);
  }, []);

  const commit = useCallback((next: BootstrapPayload) => {
    payloadRef.current = next;
    setPayload(next);
  }, []);

  const applyBackend = useCallback(
    (next: BootstrapPayload) => {
      commit(next);
      if (next.migration && next.migration.created_slots > 0) {
        pushToast("info", `已从旧配置迁移 ${next.migration.created_slots} 个端口槽位`);
      }
    },
    [commit, pushToast],
  );

  // Browser-preview helper: mutate settings then recompute slot views locally.
  const applyPreview = useCallback(
    (mutate: (settings: AppSettings) => AppSettings) => {
      const current = payloadRef.current;
      if (!current) {
        return;
      }
      const nextSettings = mutate(current.settings);
      commit({
        ...current,
        settings: nextSettings,
        slots: buildSlotViews(nextSettings),
      });
    },
    [commit],
  );

  const loadInitialData = useCallback(async () => {
    setBootstrapping(true);
    setBootstrapError(null);
    try {
      if (browserPreview) {
        const preview = createPreviewPayload();
        commit(preview);
        setLogs([{ level: "info", message: "浏览器预览模式：所有操作仅作可视化演示。", timestamp: timestamp() }]);
        return;
      }
      const data = await bootstrap();
      applyBackend(data);
    } catch (error) {
      const message = errorMessage(error);
      appendLog("error", message);
      setBootstrapError(message);
    } finally {
      setBootstrapping(false);
    }
  }, [browserPreview, commit, applyBackend, appendLog]);

  useEffect(() => {
    void loadInitialData();
  }, [loadInitialData]);

  useEffect(() => {
    if (browserPreview) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenLogs((entry) => {
      setLogs((prev) => [...prev.slice(-299), entry]);
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [browserPreview]);

  const running = payload?.runtime.running ?? false;

  // Desktop: poll the per-port traffic report while the core is running.
  useEffect(() => {
    if (browserPreview) {
      return;
    }
    if (!running) {
      setPortTraffic(null);
      setPortTrafficError(null);
      return;
    }
    let active = true;
    let inFlight = false;
    const tick = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const report = await getPortTraffic();
        if (active) {
          setPortTraffic(report);
          setPortTrafficError(null);
          if (!report.running) {
            setPayload((current) =>
              current ? { ...current, runtime: { ...current.runtime, running: false } } : current,
            );
            appendLog("warn", "检测到 mihomo 已意外退出");
          }
        }
      } catch (error) {
        if (active) {
          setPortTraffic(null);
          setPortTrafficError(errorMessage(error));
        }
      } finally {
        inFlight = false;
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 1500);
    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, [appendLog, browserPreview, running]);

  // Browser preview: synthesize coherent per-port traffic from current slots.
  useEffect(() => {
    if (!browserPreview) {
      return;
    }
    if (!payload || !payload.runtime.running) {
      setPortTraffic(null);
      return;
    }
    setPortTraffic(buildPreviewTraffic(payload.settings, payload.slots));
  }, [browserPreview, payload]);

  // Generic wrapper for backend mutations with busy + error handling.
  const run = useCallback(
    async (action: () => Promise<BootstrapPayload>, errorPrefix?: string) => {
      if (mutationInFlight.current) {
        return false;
      }
      mutationInFlight.current = true;
      setBusy(true);
      try {
        const next = await action();
        applyBackend(next);
        return true;
      } catch (error) {
        const message = errorMessage(error);
        appendLog("error", message);
        pushToast("error", errorPrefix ? `${errorPrefix}：${message}` : message);
        return false;
      } finally {
        mutationInFlight.current = false;
        setBusy(false);
      }
    },
    [applyBackend, appendLog, pushToast],
  );

  const createSlot = useCallback(
    async (input: PortSlotInput) => {
      if (browserPreview) {
        const conflict = payloadRef.current?.settings.port_slots.some(
          (slot) => slot.local_port === input.local_port,
        );
        if (conflict) {
          pushToast("error", `端口 ${input.local_port} 已被占用`);
          return false;
        }
        applyPreview((settings) => ({
          ...settings,
          port_slots: [
            ...settings.port_slots,
            {
              id: makeId(),
              name: input.name.trim() || `端口 ${input.local_port}`,
              note: input.note.trim(),
              local_port: input.local_port,
              enabled: input.enabled,
              binding: input.binding
                ? (() => {
                    const sub = settings.subscriptions.find((item) => item.id === input.binding?.sub_id);
                    const node = sub?.nodes[input.binding.node_index];
                    return sub && node
                      ? { sub_id: sub.id, fingerprint: `${node.type}|${node.server}|${node.port}`, node_name: node.name }
                      : null;
                  })()
                : null,
            },
          ],
        }));
        pushToast("success", "端口已创建");
        return true;
      }
      const ok = await run(() => createPortSlot(input), "创建端口失败");
      if (ok) {
        pushToast("success", "端口已创建");
      }
      return ok;
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const updateSlot = useCallback(
    async (slotId: string, input: PortSlotInput) => {
      if (browserPreview) {
        const conflict = payloadRef.current?.settings.port_slots.some(
          (slot) => slot.local_port === input.local_port && slot.id !== slotId,
        );
        if (conflict) {
          pushToast("error", `端口 ${input.local_port} 已被占用`);
          return false;
        }
        applyPreview((settings) => ({
          ...settings,
          port_slots: settings.port_slots.map((slot) =>
            slot.id === slotId
              ? {
                  ...slot,
                  name: input.name.trim() || `端口 ${input.local_port}`,
                  note: input.note.trim(),
                  local_port: input.local_port,
                  enabled: input.enabled,
                  binding: input.binding
                    ? (() => {
                        const sub = settings.subscriptions.find((item) => item.id === input.binding?.sub_id);
                        const node = sub?.nodes[input.binding.node_index];
                        return sub && node
                          ? { sub_id: sub.id, fingerprint: `${node.type}|${node.server}|${node.port}`, node_name: node.name }
                          : slot.binding;
                      })()
                    : null,
                }
              : slot,
          ),
        }));
        pushToast("success", "端口已更新");
        return true;
      }
      const ok = await run(() => updatePortSlot(slotId, input), "更新端口失败");
      if (ok) {
        pushToast("success", "端口已更新");
      }
      return ok;
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const deleteSlot = useCallback(
    async (slotId: string) => {
      if (browserPreview) {
        applyPreview((settings) => ({
          ...settings,
          port_slots: settings.port_slots.filter((slot) => slot.id !== slotId),
        }));
        pushToast("success", "端口已删除");
        return true;
      }
      const ok = await run(() => deletePortSlot(slotId), "删除端口失败");
      if (ok) {
        pushToast("success", "端口已删除");
      }
      return ok;
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const toggleSlot = useCallback(
    async (slotId: string, enabled: boolean) => {
      if (browserPreview) {
        applyPreview((settings) => ({
          ...settings,
          port_slots: settings.port_slots.map((slot) =>
            slot.id === slotId ? { ...slot, enabled } : slot,
          ),
        }));
        return true;
      }
      return run(() => setPortSlotEnabled(slotId, enabled), "切换端口状态失败");
    },
    [browserPreview, applyPreview, run],
  );

  const bindSlot = useCallback(
    async (slotId: string, binding: PortSlotBindingInput) => {
      if (browserPreview) {
        applyPreview((settings) => ({
          ...settings,
          port_slots: settings.port_slots.map((slot) => {
            if (slot.id !== slotId) {
              return slot;
            }
            const sub = settings.subscriptions.find((item) => item.id === binding.sub_id);
            const node = sub?.nodes[binding.node_index];
            return sub && node
              ? {
                  ...slot,
                  binding: { sub_id: sub.id, fingerprint: `${node.type}|${node.server}|${node.port}`, node_name: node.name },
                }
              : slot;
          }),
        }));
        pushToast("success", "已更新端口绑定");
        return true;
      }
      const ok = await run(() => bindPortSlot(slotId, binding), "绑定节点失败");
      if (ok) {
        pushToast("success", "已更新端口绑定");
      }
      return ok;
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const bindSlotsBatch = useCallback(
    async (assignments: { slotId: string; subId: string; nodeIndex: number }[]) => {
      if (assignments.length === 0) {
        return false;
      }
      if (browserPreview) {
        applyPreview((settings) => ({
          ...settings,
          port_slots: settings.port_slots.map((slot) => {
            const assignment = assignments.find((item) => item.slotId === slot.id);
            if (!assignment) {
              return slot;
            }
            const sub = settings.subscriptions.find((item) => item.id === assignment.subId);
            const node = sub?.nodes[assignment.nodeIndex];
            return sub && node
              ? { ...slot, binding: { sub_id: sub.id, fingerprint: `${node.type}|${node.server}|${node.port}`, node_name: node.name } }
              : slot;
          }),
        }));
        pushToast("success", `已重新绑定 ${assignments.length} 个端口`);
        return true;
      }
      const ok = await run(
        () =>
          bindPortSlotsBatch(
            assignments.map((assignment) => ({
              slot_id: assignment.slotId,
              sub_id: assignment.subId,
              node_index: assignment.nodeIndex,
            })),
          ),
        "批量绑定失败",
      );
      if (ok) {
        pushToast("success", `已重新绑定 ${assignments.length} 个端口`);
      }
      return ok;
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const clearBinding = useCallback(
    async (slotId: string) => {
      if (browserPreview) {
        applyPreview((settings) => ({
          ...settings,
          port_slots: settings.port_slots.map((slot) =>
            slot.id === slotId ? { ...slot, binding: null } : slot,
          ),
        }));
        return true;
      }
      return run(() => clearPortSlotBinding(slotId), "解除绑定失败");
    },
    [browserPreview, applyPreview, run],
  );

  const reorderSlots = useCallback(
    async (orderedIds: string[]) => {
      if (browserPreview) {
        applyPreview((settings) => {
          const map = new Map(settings.port_slots.map((slot) => [slot.id, slot]));
          return {
            ...settings,
            port_slots: orderedIds
              .map((id) => map.get(id))
              .filter((slot): slot is NonNullable<typeof slot> => Boolean(slot)),
          };
        });
        return true;
      }
      return run(() => reorderPortSlots(orderedIds), "调整顺序失败");
    },
    [browserPreview, applyPreview, run],
  );

  const validatePortValue = useCallback(
    async (port: number, ignoreSlotId: string | null): Promise<PortValidation> => {
      if (browserPreview) {
        if (!Number.isFinite(port) || port < 1024 || port > 65535) {
          return { status: "invalid", message: "端口需在 1024 - 65535 之间" };
        }
        const conflict = payloadRef.current?.settings.port_slots.some(
          (slot) => slot.local_port === port && slot.id !== ignoreSlotId,
        );
        if (conflict) {
          return { status: "conflict", message: `端口 ${port} 已被其它端口槽位使用` };
        }
        return { status: "ok", message: "端口可用" };
      }
      try {
        return await validatePort(port, ignoreSlotId);
      } catch (error) {
        return { status: "invalid", message: errorMessage(error) };
      }
    },
    [browserPreview],
  );

  const startCore = useCallback(async () => {
    if (browserPreview) {
      applyPreview((settings) => settings);
      const current = payloadRef.current;
      if (current) {
        commit({ ...current, runtime: { ...current.runtime, running: true } });
      }
      pushToast("success", "预览模式：核心已切换为运行中");
      return true;
    }
    const ok = await run(() => startMihomo(), "启动失败");
    if (ok) {
      pushToast("success", "mihomo 已启动");
    }
    return ok;
  }, [browserPreview, applyPreview, commit, run, pushToast]);

  const stopCore = useCallback(async () => {
    if (browserPreview) {
      const current = payloadRef.current;
      if (current) {
        commit({ ...current, runtime: { ...current.runtime, running: false } });
      }
      pushToast("info", "预览模式：核心已停止");
      return true;
    }
    return run(() => stopMihomo(), "停止失败");
  }, [browserPreview, commit, run, pushToast]);

  const saveProxy = useCallback(
    async (enabled: boolean, url: string, mihomoPath: string) => {
      if (browserPreview) {
        applyPreview((settings) => ({
          ...settings,
          local_proxy_enabled: enabled,
          local_proxy_url: url,
          mihomo_path: mihomoPath,
        }));
        pushToast("success", "已保存设置");
        return true;
      }
      const ok = await run(() => saveProxySettings(enabled, url, mihomoPath), "保存设置失败");
      if (ok) {
        pushToast("success", "已保存设置");
      }
      return ok;
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const addSubscription = useCallback(
    async (input: UpsertSubscriptionInput) => {
      if (browserPreview) {
        applyPreview((settings) => {
          const next: SubscriptionRecord = {
            id: makeId(),
            name: input.name.trim() || `订阅${settings.subscriptions.length + 1}`,
            url: input.url?.trim() ?? "",
            ua: "mihomo-switch",
            start_port: 10801,
            manual: input.manual,
            content: input.content?.trim() ?? "",
            nodes: [],
            selected_node_indices: [],
            port_assignments: {},
            node_remarks: {},
          };
          return { ...settings, subscriptions: [...settings.subscriptions, next] };
        });
        pushToast("success", "订阅已创建");
        return true;
      }
      const ok = await run(() => createSubscription(input), "创建订阅失败");
      return ok;
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const editSubscription = useCallback(
    async (subId: string, input: UpsertSubscriptionInput) => {
      if (browserPreview) {
        applyPreview((settings) => ({
          ...settings,
          subscriptions: settings.subscriptions.map((item) =>
            item.id === subId
              ? { ...item, name: input.name.trim() || item.name, url: input.url?.trim() ?? item.url, manual: input.manual, content: input.content?.trim() ?? item.content }
              : item,
          ),
        }));
        pushToast("success", "订阅已更新");
        return true;
      }
      return run(() => updateSubscription(subId, input), "更新订阅失败");
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const removeSubscription = useCallback(
    async (subId: string) => {
      if (browserPreview) {
        applyPreview((settings) => ({
          ...settings,
          subscriptions: settings.subscriptions.filter((item) => item.id !== subId),
        }));
        pushToast("success", "订阅已删除");
        return true;
      }
      const ok = await run(() => deleteSubscription(subId), "删除订阅失败");
      if (ok) {
        pushToast("success", "订阅已删除");
      }
      return ok;
    },
    [browserPreview, applyPreview, run, pushToast],
  );

  const refreshSubscription = useCallback(
    async (subId: string) => {
      if (browserPreview) {
        pushToast("info", "预览模式：未执行真实订阅更新");
        return true;
      }
      const ok = await run(() => importSubscription(subId), "更新订阅失败");
      if (ok) {
        pushToast("success", "订阅已更新");
      }
      return ok;
    },
    [browserPreview, run, pushToast],
  );

  const testNode = useCallback(
    async (subId: string, nodeIndex: number) => {
      if (latencyBatchRunning.current) {
        return;
      }
      const key = latencyKey(subId, nodeIndex);
      if (pendingLatencyKeys.current.has(key)) {
        return;
      }

      const queued = pendingLatencyKeys.current.size > 0;
      pendingLatencyKeys.current.add(key);
      setLatencyMap((prev) => ({ ...prev, [key]: queued ? "排队中..." : "测试中..." }));

      const task = latencyQueue.current.catch(() => undefined).then(async () => {
        setLatencyMap((prev) => ({ ...prev, [key]: "测试中..." }));
        if (browserPreview) {
          await new Promise((resolve) => window.setTimeout(resolve, 220));
          setLatencyMap((prev) => ({ ...prev, [key]: `${52 + (nodeIndex * 19) % 120} ms` }));
          return;
        }
        try {
          const [result] = await testLatency(subId, [nodeIndex]);
          setLatencyMap((prev) => ({ ...prev, [key]: result?.result ?? "失败" }));
        } catch (error) {
          appendLog("error", errorMessage(error));
          setLatencyMap((prev) => ({ ...prev, [key]: "失败" }));
        }
      });
      latencyQueue.current = task.catch(() => undefined);
      try {
        await task;
      } finally {
        pendingLatencyKeys.current.delete(key);
      }
    },
    [browserPreview, appendLog],
  );

  const cancelLatencyBatch = useCallback(() => {
    if (!latencyBatchRunning.current) {
      return;
    }
    latencyBatchCancel.current = true;
    if (!browserPreview) {
      void cancelLatency().catch((error) => appendLog("error", errorMessage(error)));
    }
  }, [appendLog, browserPreview]);

  const testBoundPortNodes = useCallback(
    async (targets: LatencyTestTarget[]) => {
      if (latencyBatchRunning.current) {
        return;
      }

      const unique = dedupeLatencyTargets(targets);
      if (unique.length === 0) {
        return;
      }

      latencyBatchRunning.current = true;
      latencyBatchCancel.current = false;
      setLatencyBatch({ running: true, done: 0, total: unique.length });

      const pendingKeys = unique.map((target) => latencyKey(target.subId, target.nodeIndex));
      setLatencyMap((prev) => {
        const next = { ...prev };
        for (const key of pendingKeys) {
          next[key] = "排队中...";
        }
        return next;
      });

      let completed = 0;
      let successCount = 0;

      const finishKeys = (keys: string[], value: string) => {
        setLatencyMap((prev) => {
          const next = { ...prev };
          for (const key of keys) {
            next[key] = value;
          }
          return next;
        });
      };

      try {
        if (browserPreview) {
          for (const target of unique) {
            if (latencyBatchCancel.current) {
              break;
            }
            const key = latencyKey(target.subId, target.nodeIndex);
            setLatencyMap((prev) => ({ ...prev, [key]: "测试中..." }));
            await new Promise((resolve) => window.setTimeout(resolve, 220));
            if (latencyBatchCancel.current) {
              break;
            }
            const result = `${52 + (target.nodeIndex * 19) % 120} ms`;
            setLatencyMap((prev) => ({ ...prev, [key]: result }));
            successCount += 1;
            completed += 1;
            setLatencyBatch({ running: true, done: completed, total: unique.length });
          }
        } else {
          const groups = groupTargetsBySubscription(unique);
          for (const [subId, nodeIndices] of groups) {
            if (latencyBatchCancel.current) {
              break;
            }

            const groupKeys = nodeIndices.map((nodeIndex) => latencyKey(subId, nodeIndex));
            setLatencyMap((prev) => {
              const next = { ...prev };
              for (const key of groupKeys) {
                next[key] = "测试中...";
              }
              return next;
            });

            try {
              const results = await testLatency(subId, nodeIndices);
              if (latencyBatchCancel.current) {
                break;
              }
              const resultByIndex = new Map(results.map((item) => [item.node_index, item.result]));
              setLatencyMap((prev) => {
                const next = { ...prev };
                for (const nodeIndex of nodeIndices) {
                  const key = latencyKey(subId, nodeIndex);
                  const result = resultByIndex.get(nodeIndex) ?? "失败";
                  next[key] = result;
                  if (result.endsWith("ms")) {
                    successCount += 1;
                  }
                }
                return next;
              });
              completed += nodeIndices.length;
              setLatencyBatch({ running: true, done: completed, total: unique.length });
            } catch (error) {
              if (latencyBatchCancel.current) {
                break;
              }
              appendLog("error", errorMessage(error));
              finishKeys(groupKeys, "失败");
              completed += nodeIndices.length;
              setLatencyBatch({ running: true, done: completed, total: unique.length });
            }
          }
        }

        if (latencyBatchCancel.current) {
          setLatencyMap((prev) => {
            const next = { ...prev };
            for (const key of pendingKeys) {
              if (next[key] === "排队中..." || next[key] === "测试中...") {
                next[key] = "--";
              }
            }
            return next;
          });
          pushToast("info", "已取消测速");
        } else {
          pushToast(
            successCount === unique.length ? "success" : "warn",
            `测速完成：${successCount}/${unique.length} 个节点成功`,
          );
        }
      } finally {
        latencyBatchRunning.current = false;
        latencyBatchCancel.current = false;
        setLatencyBatch(INITIAL_LATENCY_BATCH);
      }
    },
    [appendLog, browserPreview, pushToast],
  );

  return {
    browserPreview,
    payload,
    bootstrapError,
    bootstrapping,
    retryBootstrap: loadInitialData,
    logs,
    busy,
    latencyMap,
    latencyBatch,
    portTraffic,
    portTrafficError,
    toasts,
    pushToast,
    dismissToast,
    appendLog,
    clearLogs: () => setLogs([]),
    createSlot,
    updateSlot,
    deleteSlot,
    toggleSlot,
    bindSlot,
    bindSlotsBatch,
    clearBinding,
    reorderSlots,
    validatePortValue,
    startCore,
    stopCore,
    saveProxy,
    addSubscription,
    editSubscription,
    removeSubscription,
    refreshSubscription,
    testNode,
    testBoundPortNodes,
    cancelLatencyBatch,
  };
}

export type AppData = ReturnType<typeof useAppData>;
