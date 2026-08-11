import { useEffect, useMemo, useState } from "react";

import type { PortSlotView, PortTrafficEntry } from "../types";
import type { AppData } from "../hooks/useAppData";
import { latencyKey, type LatencyTestTarget } from "../hooks/useAppData";
import { BatchRebindDialog } from "../components/BatchRebindDialog";
import { NodePicker, type NodeChoice } from "../components/NodePicker";
import { PortFormDialog, type PortFormInitial } from "../components/PortFormDialog";
import { PortTable } from "../components/PortTable";
import { StatusPanel } from "../components/StatusPanel";
import { EmptyState } from "../components/ui/EmptyState";
import { Icon } from "../components/ui/Icon";
import { latencyMs } from "../lib/format";
import styles from "./PortsPage.module.css";

type SortKey = "name" | "port" | "latency" | "status";

type Props = {
  app: AppData;
  createRequested: boolean;
  onCreateRequestHandled: () => void;
  onOpenActivity?: (port: number) => void;
};

const STATUS_ORDER: Record<string, number> = {
  running: 0,
  ready: 1,
  invalid: 2,
  unbound: 3,
  disabled: 4,
};

function statusBucket(slot: PortSlotView, running: boolean) {
  if (!slot.enabled) return "disabled";
  if (slot.state === "unbound") return "unbound";
  if (slot.state === "invalid") return "invalid";
  return running ? "running" : "ready";
}

export function PortsPage({ app, createRequested, onCreateRequestHandled, onOpenActivity }: Props) {
  const settings = app.payload?.settings;
  const slots = app.payload?.slots ?? [];
  const running = app.payload?.runtime.running ?? false;

  const [keyword, setKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [subFilter, setSubFilter] = useState("all");
  const [sortKey, setSortKey] = useState<SortKey>("status");
  const [onlyInvalid, setOnlyInvalid] = useState(false);

  const [formOpen, setFormOpen] = useState(false);
  const [formMode, setFormMode] = useState<"create" | "edit">("create");
  const [formInitial, setFormInitial] = useState<PortFormInitial | null>(null);

  const [pickerSlot, setPickerSlot] = useState<PortSlotView | null>(null);
  const [batchOpen, setBatchOpen] = useState(false);

  useEffect(() => {
    if (!createRequested) {
      return;
    }
    setFormMode("create");
    setFormInitial(null);
    setFormOpen(true);
    onCreateRequestHandled();
  }, [createRequested, onCreateRequestHandled]);

  const invalidSlots = useMemo(
    () => slots.filter((slot) => slot.enabled && (slot.state === "invalid" || slot.state === "unbound")),
    [slots],
  );
  const invalidCount = invalidSlots.length;

  const suggestedPort = useMemo(() => {
    const used = new Set((settings?.port_slots ?? []).map((slot) => slot.local_port));
    let candidate = 10808;
    while (used.has(candidate)) {
      candidate += 1;
    }
    return candidate;
  }, [settings?.port_slots]);

  const visibleSlots = useMemo(() => {
    const lower = keyword.trim().toLowerCase();
    const filtered = slots.filter((slot) => {
      if (onlyInvalid && !(slot.state === "invalid" || slot.state === "unbound")) {
        return false;
      }
      if (statusFilter !== "all" && statusBucket(slot, running) !== statusFilter) {
        return false;
      }
      if (subFilter !== "all" && slot.binding?.sub_id !== subFilter) {
        return false;
      }
      if (lower) {
        const haystack = `${slot.name} ${slot.note} ${slot.local_port} ${slot.binding?.node_name ?? ""}`.toLowerCase();
        if (!haystack.includes(lower)) {
          return false;
        }
      }
      return true;
    });

    return [...filtered].sort((a, b) => {
      if (sortKey === "name") {
        return a.name.localeCompare(b.name, "zh-CN", { numeric: true });
      }
      if (sortKey === "port") {
        return a.local_port - b.local_port;
      }
      if (sortKey === "latency") {
        const la = a.binding?.node_index != null ? latencyMs(app.latencyMap[latencyKey(a.binding.sub_id, a.binding.node_index)]) : null;
        const lb = b.binding?.node_index != null ? latencyMs(app.latencyMap[latencyKey(b.binding.sub_id, b.binding.node_index)]) : null;
        if (la === null && lb === null) return 0;
        if (la === null) return 1;
        if (lb === null) return -1;
        return la - lb;
      }
      return STATUS_ORDER[statusBucket(a, running)] - STATUS_ORDER[statusBucket(b, running)];
    });
  }, [slots, keyword, statusFilter, subFilter, sortKey, onlyInvalid, running, app.latencyMap]);

  const latencyTestTargets = useMemo(() => {
    const targets: LatencyTestTarget[] = [];
    const seen = new Set<string>();
    for (const slot of visibleSlots) {
      if (slot.state !== "valid" || slot.binding?.node_index == null) {
        continue;
      }
      const key = latencyKey(slot.binding.sub_id, slot.binding.node_index);
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      targets.push({ subId: slot.binding.sub_id, nodeIndex: slot.binding.node_index });
    }
    return targets;
  }, [visibleSlots]);

  const latencyTesting = app.latencyBatch.running;

  const trafficByPort = useMemo(() => {
    const map: Record<number, PortTrafficEntry> = {};
    for (const entry of app.portTraffic?.ports ?? []) {
      map[entry.local_port] = entry;
    }
    return map;
  }, [app.portTraffic]);

  const totals = useMemo(() => {
    let upload = 0;
    let download = 0;
    let connections = 0;
    for (const entry of app.portTraffic?.ports ?? []) {
      upload += entry.upload;
      download += entry.download;
      connections += entry.connections;
    }
    return { upload, download, connections };
  }, [app.portTraffic]);

  const handleCopy = async (port: number) => {
    const address = `127.0.0.1:${port}`;
    try {
      await navigator.clipboard.writeText(address);
      app.pushToast("success", `已复制 ${address}`);
    } catch {
      app.pushToast("error", "复制失败，请手动复制");
    }
  };

  const openEdit = (slot: PortSlotView) => {
    setFormMode("edit");
    setFormInitial({
      id: slot.id,
      name: slot.name,
      note: slot.note,
      localPort: slot.local_port,
      enabled: slot.enabled,
      binding:
        slot.binding && slot.binding.node_index != null
          ? { subId: slot.binding.sub_id, nodeIndex: slot.binding.node_index }
          : null,
    });
    setFormOpen(true);
  };

  const pickerCurrent: NodeChoice | null =
    pickerSlot?.binding && pickerSlot.binding.node_index != null
      ? { subId: pickerSlot.binding.sub_id, nodeIndex: pickerSlot.binding.node_index }
      : null;

  return (
    <div className={styles.layout}>
      <div className={styles.main}>
        <header className={styles.header}>
          <div className={styles.headingGroup}>
            <h1 className={styles.title}>端口管理</h1>
            <p className={styles.subtitle}>每个端口是一个稳定的本地代理，可随时更换绑定的节点。</p>
          </div>
        </header>

        <div className={styles.summaryBlock}>
          {invalidCount > 0 ? (
            <div className={styles.invalidBanner}>
              <Icon name="alert" size="sm" className={styles.invalidIcon} />
              <span className={styles.invalidText}>
                有 <strong>{invalidCount}</strong> 个已启用端口需要重新绑定节点（节点失效或未绑定）。
              </span>
              <button type="button" className="btn btnSm" onClick={() => setOnlyInvalid((prev) => !prev)}>
                {onlyInvalid ? "显示全部" : "仅看失效"}
              </button>
              <button type="button" className="btn btnSm btnPrimarySoft" onClick={() => setBatchOpen(true)}>
                批量重新绑定
              </button>
            </div>
          ) : null}

          <StatusPanel
            totalUpload={totals.upload}
            totalDownload={totals.download}
            totalConnections={totals.connections}
          />
        </div>

        <div className={styles.toolbar}>
          <div className={styles.searchField}>
            <Icon name="search" size="md" className={styles.searchIcon} />
            <input
              className={`input ${styles.search}`}
              aria-label="搜索端口"
              placeholder="搜索端口名称、地址、备注或节点"
              value={keyword}
              onChange={(event) => setKeyword(event.target.value)}
            />
            {keyword ? (
              <button type="button" className={styles.clearSearch} aria-label="清空搜索" onClick={() => setKeyword("")}>
                <Icon name="close" size="sm" />
              </button>
            ) : null}
          </div>
          <div className={styles.filterRow}>
            <select aria-label="按状态筛选端口" className={`select ${styles.filter}`} value={statusFilter} onChange={(event) => setStatusFilter(event.target.value)}>
              <option value="all">全部状态</option>
              <option value="running">运行中</option>
              <option value="ready">就绪</option>
              <option value="invalid">节点失效</option>
              <option value="unbound">未绑定</option>
              <option value="disabled">已停用</option>
            </select>
            <select aria-label="按订阅筛选端口" className={`select ${styles.filter}`} value={subFilter} onChange={(event) => setSubFilter(event.target.value)}>
              <option value="all">全部订阅</option>
              {(settings?.subscriptions ?? []).map((sub) => (
                <option key={sub.id} value={sub.id}>{sub.name}</option>
              ))}
            </select>
            <select aria-label="端口排序" className={`select ${styles.filter}`} value={sortKey} onChange={(event) => setSortKey(event.target.value as SortKey)}>
              <option value="status">按状态排序</option>
              <option value="port">按端口排序</option>
              <option value="latency">按延迟排序</option>
              <option value="name">按名称排序</option>
            </select>
            <div className={styles.toolbarActions}>
              <button
                type="button"
                className={`btn btnSm ${styles.batchTestBtn}`}
                disabled={latencyTesting || latencyTestTargets.length === 0}
                title={
                  latencyTesting
                    ? `正在测速 ${app.latencyBatch.done}/${app.latencyBatch.total}`
                    : latencyTestTargets.length === 0
                      ? "当前列表没有可测速的绑定节点"
                      : undefined
                }
                onClick={() => {
                  if (!latencyTesting) {
                    void app.testBoundPortNodes(latencyTestTargets);
                  }
                }}
              >
                <Icon name="zap" size="sm" />
                <span className={styles.batchTestLabel}>
                  {latencyTesting ? `测速中 ${app.latencyBatch.done}/${app.latencyBatch.total}` : "全部测速"}
                </span>
              </button>
            </div>
          </div>
        </div>

        <div className={styles.tableCard}>
          {slots.length === 0 ? (
            <EmptyState
              icon={<Icon name="ports" size="lg" />}
              title="还没有任何端口"
              description="创建一个固定的本地代理端口，并为它绑定一个订阅节点，指纹浏览器即可连接它。"
              action={
                <button className="btn btnPrimary" onClick={() => { setFormMode("create"); setFormInitial(null); setFormOpen(true); }}>
                  新增端口
                </button>
              }
            />
          ) : visibleSlots.length === 0 ? (
            <EmptyState icon={<Icon name="empty" size="lg" />} title="没有符合条件的端口" description="尝试调整筛选条件或清空搜索关键词。" />
          ) : (
            <PortTable
              slots={visibleSlots}
              latencyMap={app.latencyMap}
              trafficByPort={trafficByPort}
              running={running}
              latencyTesting={latencyTesting}
              onToggle={(slot, enabled) => void app.toggleSlot(slot.id, enabled)}
              onCopyAddress={handleCopy}
              onChangeNode={(slot) => setPickerSlot(slot)}
              onTest={(slot) => {
                if (slot.binding && slot.binding.node_index != null) {
                  void app.testNode(slot.binding.sub_id, slot.binding.node_index);
                }
              }}
              onEdit={openEdit}
              onDelete={(slot) => void app.deleteSlot(slot.id)}
              onTrafficDetail={(slot) => onOpenActivity?.(slot.local_port)}
            />
          )}
        </div>
      </div>

      <PortFormDialog
        open={formOpen}
        mode={formMode}
        initial={formInitial}
        subscriptions={settings?.subscriptions ?? []}
        latencyMap={app.latencyMap}
        busy={app.busy}
        suggestedPort={suggestedPort}
        onTest={(subId, nodeIndex) => void app.testNode(subId, nodeIndex)}
        validatePort={app.validatePortValue}
        onClose={() => setFormOpen(false)}
        onSubmit={async (input) => {
          const ok =
            formMode === "edit" && formInitial
              ? await app.updateSlot(formInitial.id, input)
              : await app.createSlot(input);
          if (ok) {
            setFormOpen(false);
          }
        }}
      />

      {pickerSlot ? (
        <NodePicker
          open={Boolean(pickerSlot)}
          title={pickerSlot.state === "valid" ? "更换节点" : "重新绑定节点"}
          portName={pickerSlot.name}
          localPort={pickerSlot.local_port}
          subscriptions={settings?.subscriptions ?? []}
          currentNodeName={pickerSlot.binding?.node_name ?? null}
          current={pickerCurrent}
          latencyMap={app.latencyMap}
          onTest={(subId, nodeIndex) => void app.testNode(subId, nodeIndex)}
          onCancel={() => setPickerSlot(null)}
          onConfirm={async (choice) => {
            const ok = await app.bindSlot(pickerSlot.id, { sub_id: choice.subId, node_index: choice.nodeIndex });
            if (ok) {
              setPickerSlot(null);
            }
          }}
        />
      ) : null}

      <BatchRebindDialog
        open={batchOpen}
        busy={app.busy}
        slots={invalidSlots}
        subscriptions={settings?.subscriptions ?? []}
        latencyMap={app.latencyMap}
        onTest={(subId, nodeIndex) => void app.testNode(subId, nodeIndex)}
        onCancel={() => setBatchOpen(false)}
        onApply={async (assignments) => {
          const ok = await app.bindSlotsBatch(assignments);
          if (ok) {
            setBatchOpen(false);
          }
        }}
      />

    </div>
  );
}
