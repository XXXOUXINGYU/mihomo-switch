import { useEffect, useRef, useState } from "react";

import type { PortSlotView, PortTrafficEntry } from "../types";
import type { LatencyMap } from "../hooks/useAppData";
import { latencyKey } from "../hooks/useAppData";
import { detectRegion } from "../lib/region";
import { formatBytes, formatSpeed, latencyTone, protocolLabel, proxyAddress } from "../lib/format";
import { Icon } from "./ui/Icon";
import styles from "./PortTable.module.css";

export type SlotMetrics = {
  latency: string;
  connections: number;
  upload: number;
  download: number;
  uploadSpeed: number;
  downloadSpeed: number;
  active: boolean;
};

type Props = {
  slots: PortSlotView[];
  latencyMap: LatencyMap;
  trafficByPort: Record<number, PortTrafficEntry>;
  running: boolean;
  latencyTesting?: boolean;
  onToggle: (slot: PortSlotView, enabled: boolean) => void;
  onCopyAddress: (port: number) => void;
  onChangeNode: (slot: PortSlotView) => void;
  onTest: (slot: PortSlotView) => void;
  onEdit: (slot: PortSlotView) => void;
  onDelete: (slot: PortSlotView) => void;
  onTrafficDetail: (slot: PortSlotView) => void;
};

const toneClass: Record<string, string> = {
  good: styles.latGood,
  warn: styles.latWarn,
  bad: styles.latBad,
  pending: styles.latPending,
  idle: styles.latIdle,
};

function statusOf(slot: PortSlotView, running: boolean) {
  if (!slot.enabled) {
    return { label: "已停用", cls: "tagMuted", dot: "dotMuted" };
  }
  if (slot.state === "unbound") {
    return { label: "未绑定", cls: "tagWarn", dot: "dotWarn" };
  }
  if (slot.state === "invalid") {
    return { label: "已失效", cls: "tagWarn", dot: "dotWarn" };
  }
  if (running) {
    return { label: "运行中", cls: "tagGood", dot: "dotLive" };
  }
  return { label: "就绪", cls: "tagInfo", dot: "dotMuted" };
}

export function PortTable(props: Props) {
  const [menuId, setMenuId] = useState<string | null>(null);
  const bodyRef = useRef<HTMLTableSectionElement>(null);
  const menuButtonRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  const closeMenu = (restoreFocus = true) => {
    const previousId = menuId;
    setMenuId(null);
    if (restoreFocus && previousId) {
      window.setTimeout(() => menuButtonRefs.current[previousId]?.focus(), 0);
    }
  };

  useEffect(() => {
    if (!menuId) {
      return;
    }
    const menu = document.getElementById(`port-menu-${menuId}`);
    menu?.querySelector<HTMLButtonElement>("button")?.focus();
    const close = () => closeMenu(false);
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closeMenu();
      }
    };
    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [menuId]);

  const metricsFor = (slot: PortSlotView): SlotMetrics => {
    const key = slot.binding?.node_index != null && slot.binding.sub_id
      ? latencyKey(slot.binding.sub_id, slot.binding.node_index)
      : null;
    const latency = key ? props.latencyMap[key] ?? "--" : "--";
    const entry = props.trafficByPort[slot.local_port];
    const active = Boolean(props.running && slot.enabled && slot.state === "valid");
    return {
      latency,
      connections: entry?.connections ?? 0,
      upload: entry?.upload ?? 0,
      download: entry?.download ?? 0,
      uploadSpeed: entry?.upload_speed ?? 0,
      downloadSpeed: entry?.download_speed ?? 0,
      active,
    };
  };

  const moveFocus = (index: number, delta: number) => {
    const rows = bodyRef.current?.querySelectorAll<HTMLTableRowElement>("tr[data-row]");
    if (!rows) return;
    const next = rows[index + delta];
    next?.focus();
  };

  return (
    <div className={styles.tableWrap}>
      <table className={styles.table}>
        <colgroup>
          <col className={styles.colEnable} />
          <col className={styles.colPort} />
          <col className={styles.colNode} />
          <col className={styles.colLatency} />
          <col className={styles.colActivity} />
          <col className={styles.colActions} />
        </colgroup>
        <thead>
          <tr>
            <th>启用</th>
            <th>端口</th>
            <th>绑定节点</th>
            <th className={styles.latencyColumn}>延迟</th>
            <th className={styles.activityColumn}>连接 · 流量</th>
            <th className={styles.actionsColumn}>操作</th>
          </tr>
        </thead>
        <tbody ref={bodyRef}>
          {props.slots.map((slot, index) => {
            const metrics = metricsFor(slot);
            const tone = latencyTone(metrics.latency);
            const status = statusOf(slot, props.running);
            const region = slot.binding && slot.state === "valid" ? detectRegion(slot.binding.node_name) : null;
            const invalid = slot.state === "invalid" || slot.state === "unbound";
            return (
              <tr
                key={slot.id}
                data-row
                tabIndex={0}
                className={invalid ? styles.rowFlagged : undefined}
                onKeyDown={(event) => {
                  if (event.key === "ArrowDown") {
                    event.preventDefault();
                    moveFocus(index, 1);
                  } else if (event.key === "ArrowUp") {
                    event.preventDefault();
                    moveFocus(index, -1);
                  } else if (event.key === "Enter") {
                    event.preventDefault();
                    props.onChangeNode(slot);
                  }
                }}
              >
                <td className={styles.enableCell}>
                  <button
                    type="button"
                    className={slot.enabled ? "switch switchOn" : "switch"}
                    role="switch"
                    aria-checked={slot.enabled}
                    aria-label={`启用 ${slot.name}`}
                    onClick={() => props.onToggle(slot, !slot.enabled)}
                  />
                </td>
                <td className={styles.contentCell}>
                  <div className={`${styles.portCell}${slot.note ? ` ${styles.portCellWithNote}` : ""}`}>
                    <span className={styles.name}>{slot.name}</span>
                    <button
                      type="button"
                      className={styles.address}
                      title="点击复制代理地址"
                      onClick={() => props.onCopyAddress(slot.local_port)}
                    >
                      <span className="mono">{proxyAddress(slot.local_port)}</span>
                      <Icon name="copy" size="sm" className={styles.copyIcon} />
                    </button>
                    {slot.note ? <span className={styles.note}>{slot.note}</span> : null}
                  </div>
                </td>
                <td className={styles.contentCell}>
                  {slot.state === "valid" && slot.binding ? (
                    <div className={styles.nodeCell}>
                      <div className={styles.nodeBody}>
                        <button
                          type="button"
                          className={styles.nodeButton}
                          onClick={() => props.onChangeNode(slot)}
                          title={slot.binding.node_name}
                        >
                          {slot.binding.node_name}
                        </button>
                        <div className={styles.nodeMeta}>
                          {region ? (
                            <span className={styles.region}>
                              <span aria-hidden="true">{region.flag}</span>
                              <span>{region.label}</span>
                            </span>
                          ) : null}
                          {region ? (
                            <span className={styles.metaSep} aria-hidden="true">
                              ·
                            </span>
                          ) : null}
                          <span className={styles.protocol} title={protocolLabel(slot.binding.node_type)}>
                            {protocolLabel(slot.binding.node_type)}
                          </span>
                        </div>
                      </div>
                      <span className={`tag ${status.cls} ${styles.statusTag}`}>
                        <span className={`dot ${status.dot}`} aria-hidden="true" />
                        {status.label}
                      </span>
                    </div>
                  ) : (
                    <div className={styles.nodeCell}>
                      <div className={`${styles.nodeBody} ${styles.nodeBodyFlagged}`}>
                        <button
                          type="button"
                          className={`${styles.nodeButton} ${styles.nodeButtonFlagged}`}
                          onClick={() => props.onChangeNode(slot)}
                          title={slot.state === "unbound" ? "点击选择节点" : "点击重新绑定节点"}
                        >
                          {slot.state === "unbound" ? "尚未选择节点" : slot.invalid_reason ?? "原节点已失效"}
                        </button>
                      </div>
                      <span className={`tag ${status.cls} ${styles.statusTag}`}>
                        <span className={`dot ${status.dot}`} aria-hidden="true" />
                        {status.label}
                      </span>
                    </div>
                  )}
                </td>
                <td className={`mono ${styles.latencyColumn} ${toneClass[tone]}`}>{metrics.latency}</td>
                <td className={`mono ${styles.activityColumn}`}>
                  {metrics.active ? (
                    <div className={styles.activity}>
                      <span className={styles.connections}>{metrics.connections} 个连接</span>
                      <span className={styles.up}>
                        <Icon name="arrowUp" size="sm" /> {metrics.uploadSpeed > 0 ? formatSpeed(metrics.uploadSpeed) : formatBytes(metrics.upload)}
                      </span>
                      <span className={styles.down}>
                        <Icon name="arrowDown" size="sm" /> {metrics.downloadSpeed > 0 ? formatSpeed(metrics.downloadSpeed) : formatBytes(metrics.download)}
                      </span>
                    </div>
                  ) : (
                    <span className={styles.muted}>--</span>
                  )}
                </td>
                <td className={styles.actionsColumn}>
                  <div className={styles.actions}>
                    <button
                      type="button"
                      className={`iconBtn ${styles.testAction}`}
                      title="测速"
                      aria-label="测速"
                      disabled={slot.state !== "valid" || props.latencyTesting}
                      onClick={() => props.onTest(slot)}
                    >
                      <Icon name="zap" size="md" />
                    </button>
                    <button type="button" className={`iconBtn ${styles.editAction}`} title="编辑端口" aria-label="编辑端口" onClick={() => props.onEdit(slot)}>
                      <Icon name="pencil" size="md" />
                    </button>
                    <button
                      type="button"
                      className={`iconBtn ${styles.deleteAction}`}
                      title="删除端口"
                      aria-label={`删除端口 ${slot.name}`}
                      onClick={() => props.onDelete(slot)}
                    >
                      <Icon name="trash" size="md" />
                    </button>
                    <div className={styles.menuAnchor}>
                      <button
                        ref={(element) => {
                          menuButtonRefs.current[slot.id] = element;
                        }}
                        type="button"
                        className={`iconBtn ${styles.moreAction}`}
                        title="更多操作"
                        aria-label="更多操作"
                        aria-haspopup="menu"
                        aria-expanded={menuId === slot.id}
                        aria-controls={menuId === slot.id ? `port-menu-${slot.id}` : undefined}
                        onClick={(event) => {
                          event.stopPropagation();
                          if (menuId === slot.id) {
                            closeMenu();
                          } else {
                            setMenuId(slot.id);
                          }
                        }}
                      >
                        <Icon name="more" size="md" />
                      </button>
                      {menuId === slot.id ? (
                        <div id={`port-menu-${slot.id}`} role="menu" className={styles.menu} onClick={(event) => event.stopPropagation()}>
                          <button role="menuitem" type="button" className={styles.menuItem} onClick={() => { props.onTrafficDetail(slot); closeMenu(); }}>
                            流量详情
                          </button>
                          <button role="menuitem" type="button" className={styles.menuItem} onClick={() => { props.onChangeNode(slot); closeMenu(); }}>
                            更换节点
                          </button>
                        </div>
                      ) : null}
                    </div>
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
