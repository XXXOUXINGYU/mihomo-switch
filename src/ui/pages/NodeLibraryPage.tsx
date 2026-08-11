import { useMemo, useState } from "react";

import type { AppData } from "../hooks/useAppData";
import { latencyKey, type LatencyTestTarget } from "../hooks/useAppData";
import { SlotPickerDialog } from "../components/SlotPickerDialog";
import { EmptyState } from "../components/ui/EmptyState";
import { Icon } from "../components/ui/Icon";
import { detectRegion } from "../lib/region";
import { latencyTone, protocolLabel } from "../lib/format";
import common from "./common.module.css";
import styles from "./NodeLibraryPage.module.css";

type Props = {
  app: AppData;
};

const toneClass: Record<string, string> = {
  good: styles.latGood,
  warn: styles.latWarn,
  bad: styles.latBad,
  pending: styles.latPending,
  idle: styles.latIdle,
};

export function NodeLibraryPage({ app }: Props) {
  const settings = app.payload?.settings;
  const subscriptions = settings?.subscriptions ?? [];
  const slots = app.payload?.slots ?? [];

  const [keyword, setKeyword] = useState("");
  const [subFilter, setSubFilter] = useState("all");
  const [bindTarget, setBindTarget] = useState<{ subId: string; nodeIndex: number; name: string } | null>(null);

  const nodes = useMemo(() => {
    const list = subscriptions.flatMap((sub) =>
      sub.nodes.map((node, nodeIndex) => {
        const region = detectRegion(node.name);
        const usedBy = slots.filter(
          (slot) =>
            slot.binding?.sub_id === sub.id && slot.binding.node_index === nodeIndex,
        ).length;
        return { subId: sub.id, subName: sub.name, nodeIndex, node, region, usedBy };
      }),
    );
    const lower = keyword.trim().toLowerCase();
    return list.filter((item) => {
      if (subFilter !== "all" && item.subId !== subFilter) return false;
      if (lower && !`${item.node.name} ${item.node.server} ${item.node.type}`.toLowerCase().includes(lower)) {
        return false;
      }
      return true;
    });
  }, [subscriptions, slots, keyword, subFilter]);

  const latencyTestTargets = useMemo<LatencyTestTarget[]>(
    () => nodes.map((item) => ({ subId: item.subId, nodeIndex: item.nodeIndex })),
    [nodes],
  );
  const latencyTesting = app.latencyBatch.running;

  const suggestedPort = useMemo(() => {
    const used = new Set((settings?.port_slots ?? []).map((slot) => slot.local_port));
    let candidate = 10808;
    while (used.has(candidate)) {
      candidate += 1;
    }
    return candidate;
  }, [settings?.port_slots]);

  const totalNodes = subscriptions.reduce((sum, sub) => sum + sub.nodes.length, 0);

  return (
    <div className={common.page}>
      <header className={common.header}>
        <div className={common.headingGroup}>
          <h1 className={common.title}>节点库</h1>
          <p className={common.subtitle}>浏览所有订阅节点。可测速、查看被哪些端口使用，或基于节点快速创建端口。</p>
        </div>
      </header>

      <div className={`${common.toolbar} ${styles.toolbar}`}>
        <input
          className={`input ${common.search} ${styles.search}`}
          aria-label="搜索节点"
          placeholder="搜索节点名称、地址或协议…"
          value={keyword}
          onChange={(event) => setKeyword(event.target.value)}
        />
        <select aria-label="按订阅筛选节点" className={`select ${common.filter} ${styles.filter}`} value={subFilter} onChange={(event) => setSubFilter(event.target.value)}>
          <option value="all">全部订阅</option>
          {subscriptions.map((sub) => (
            <option key={sub.id} value={sub.id}>{sub.name}</option>
          ))}
        </select>
        <button
          type="button"
          className={`btn btnSm ${styles.latencyTestBtn}`}
          disabled={latencyTesting || latencyTestTargets.length === 0}
          title={
            latencyTesting
              ? `正在测速 ${app.latencyBatch.done}/${app.latencyBatch.total}`
              : latencyTestTargets.length === 0
                ? "当前列表没有可测速的节点"
                : undefined
          }
          onClick={() => {
            if (!latencyTesting) {
              void app.testBoundPortNodes(latencyTestTargets);
            }
          }}
        >
          <Icon name="zap" size="sm" />
          <span className={styles.latencyTestLabel}>
            {latencyTesting ? `测速中 ${app.latencyBatch.done}/${app.latencyBatch.total}` : "延迟测试"}
          </span>
        </button>
      </div>

      <div className={common.scroll}>
        {totalNodes === 0 ? (
          <EmptyState icon={<Icon name="nodes" size="lg" />} title="节点库为空" description="先在订阅管理中添加订阅，节点会自动出现在这里。" />
        ) : nodes.length === 0 ? (
          <EmptyState icon={<Icon name="empty" size="lg" />} title="没有匹配的节点" description="尝试调整搜索或订阅筛选。" />
        ) : (
          <ul className={styles.list}>
            {nodes.map((item) => {
              const key = latencyKey(item.subId, item.nodeIndex);
              const latency = app.latencyMap[key] ?? "--";
              const tone = latencyTone(latency);
              return (
                <li key={`${item.subId}-${item.nodeIndex}`} className={styles.row}>
                  <span className={styles.flag} aria-hidden="true">{item.region.flag}</span>
                  <div className={styles.main}>
                    <span className={styles.name}>{item.node.name}</span>
                    <span className={styles.meta}>
                      {item.subName} · {protocolLabel(item.node.type)} · {item.region.label}
                      {item.usedBy > 0 ? ` · 被 ${item.usedBy} 个端口使用` : ""}
                    </span>
                  </div>
                  <span className={`mono ${styles.latency} ${toneClass[tone]}`}>{latency}</span>
                  <button className="btn btnSm" onClick={() => void app.testNode(item.subId, item.nodeIndex)}>测速</button>
                  <button
                    className="btn btnSm"
                    disabled={(settings?.port_slots.length ?? 0) === 0}
                    onClick={() => setBindTarget({ subId: item.subId, nodeIndex: item.nodeIndex, name: item.node.name })}
                  >
                    绑定到端口
                  </button>
                  <button
                    className="btn btnSm btnPrimary"
                    disabled={app.busy}
                    onClick={() =>
                      void app.createSlot({
                        name: item.node.name,
                        local_port: suggestedPort,
                        note: "",
                        enabled: true,
                        binding: { sub_id: item.subId, node_index: item.nodeIndex },
                      })
                    }
                  >
                    新建端口
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <SlotPickerDialog
        open={Boolean(bindTarget)}
        busy={app.busy}
        nodeName={bindTarget?.name ?? ""}
        slots={app.payload?.slots ?? []}
        onCancel={() => setBindTarget(null)}
        onPick={async (slotId) => {
          if (bindTarget) {
            const ok = await app.bindSlot(slotId, { sub_id: bindTarget.subId, node_index: bindTarget.nodeIndex });
            if (ok) {
              setBindTarget(null);
            }
          }
        }}
      />
    </div>
  );
}
