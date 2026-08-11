import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";

import type { PortConnection } from "../types";
import type { AppData } from "../hooks/useAppData";
import { EmptyState } from "../components/ui/EmptyState";
import { Icon } from "../components/ui/Icon";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { formatBytes, formatSpeed, proxyAddress } from "../lib/format";
import common from "./common.module.css";
import styles from "./ActivityPage.module.css";

type Props = {
  app: AppData;
  focusPort: number | null;
  onClearFocus: () => void;
};

type Tab = "connections" | "logs";
const tabs: Tab[] = ["connections", "logs"];
const LOG_SCROLL_BOTTOM_THRESHOLD = 24;

function isNearScrollBottom(element: HTMLElement) {
  return element.scrollHeight - element.scrollTop - element.clientHeight <= LOG_SCROLL_BOTTOM_THRESHOLD;
}

const levelClass: Record<string, string> = {
  error: styles.error,
  warn: styles.warn,
  info: styles.info,
};

export function ActivityPage({ app, focusPort, onClearFocus }: Props) {
  const [tab, setTab] = useState<Tab>("connections");
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);
  const [stickToBottom, setStickToBottom] = useState(true);
  const logsPanelRef = useRef<HTMLDivElement>(null);
  const logs = app.logs;
  const running = app.payload?.runtime.running ?? false;
  const connections = app.portTraffic?.connections ?? [];
  const selectTabByKey = (event: KeyboardEvent<HTMLButtonElement>, current: Tab) => {
    const currentIndex = tabs.indexOf(current);
    let nextIndex = currentIndex;
    if (event.key === "ArrowRight") nextIndex = (currentIndex + 1) % tabs.length;
    else if (event.key === "ArrowLeft") nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = tabs.length - 1;
    else return;
    event.preventDefault();
    const next = tabs[nextIndex];
    setTab(next);
    event.currentTarget.parentElement
      ?.querySelector<HTMLButtonElement>(`#activity-tab-${next}`)
      ?.focus();
  };

  const portName = useMemo(() => {
    const map = new Map<number, string>();
    for (const slot of app.payload?.slots ?? []) {
      map.set(slot.local_port, slot.name);
    }
    return map;
  }, [app.payload?.slots]);

  const groups = useMemo(() => {
    const byPort = new Map<number, PortConnection[]>();
    for (const conn of connections) {
      if (focusPort !== null && conn.local_port !== focusPort) {
        continue;
      }
      const list = byPort.get(conn.local_port) ?? [];
      list.push(conn);
      byPort.set(conn.local_port, list);
    }
    return [...byPort.entries()]
      .map(([port, items]) => ({ port, items }))
      .sort((a, b) => a.port - b.port);
  }, [connections, focusPort]);

  const scrollLogsToBottom = (behavior: ScrollBehavior = "auto") => {
    const panel = logsPanelRef.current;
    if (!panel) {
      return;
    }
    if (typeof panel.scrollTo === "function") {
      panel.scrollTo({ top: panel.scrollHeight, behavior });
      return;
    }
    panel.scrollTop = panel.scrollHeight;
  };

  const handleLogsScroll = () => {
    const panel = logsPanelRef.current;
    if (!panel) {
      return;
    }
    setStickToBottom(isNearScrollBottom(panel));
  };

  const jumpLogsToBottom = () => {
    setStickToBottom(true);
    scrollLogsToBottom("smooth");
  };

  useEffect(() => {
    if (tab !== "logs") {
      return;
    }
    setStickToBottom(true);
    requestAnimationFrame(() => scrollLogsToBottom());
  }, [tab]);

  useLayoutEffect(() => {
    if (tab !== "logs" || !stickToBottom) {
      return;
    }
    scrollLogsToBottom();
  }, [tab, logs, stickToBottom]);

  return (
    <div className={common.page}>
      <header className={common.header}>
        <div className={common.headingGroup}>
          <h1 className={common.title}>连接活动</h1>
          <p className={common.subtitle}>查看每个端口的实时连接明细与核心运行日志。</p>
        </div>
        <div className={styles.segment} role="tablist" aria-label="连接活动视图">
          <button
            id="activity-tab-connections"
            type="button"
            role="tab"
            aria-selected={tab === "connections"}
            aria-controls="activity-panel-connections"
            tabIndex={tab === "connections" ? 0 : -1}
            className={tab === "connections" ? styles.segActive : styles.seg}
            onClick={() => setTab("connections")}
            onKeyDown={(event) => selectTabByKey(event, "connections")}
          >
            实时连接{connections.length > 0 ? ` · ${connections.length}` : ""}
          </button>
          <button
            id="activity-tab-logs"
            type="button"
            role="tab"
            aria-selected={tab === "logs"}
            aria-controls="activity-panel-logs"
            tabIndex={tab === "logs" ? 0 : -1}
            className={tab === "logs" ? styles.segActive : styles.seg}
            onClick={() => setTab("logs")}
            onKeyDown={(event) => selectTabByKey(event, "logs")}
          >
            运行日志
          </button>
        </div>
        {tab === "logs" ? (
          <button className="btn" disabled={app.logs.length === 0} onClick={() => setClearConfirmOpen(true)}>清空日志</button>
        ) : focusPort !== null ? (
          <button className="btn" onClick={onClearFocus}>查看全部端口</button>
        ) : null}
      </header>

      {tab === "connections" ? (
        <div
          id="activity-panel-connections"
          className={common.scroll}
          role="tabpanel"
          aria-labelledby="activity-tab-connections"
        >
          {!running ? (
            <EmptyState icon={<Icon name="empty" size="lg" />} title="核心未运行" description="启动核心后，这里会按端口显示实时连接。" />
          ) : app.portTrafficError ? (
            <EmptyState icon={<Icon name="alert" size="lg" />} title="连接数据暂时不可用" description="无法读取核心连接数据，应用会自动重试。" />
          ) : groups.length === 0 ? (
            <EmptyState
              icon={<Icon name="activity" size="lg" />}
              title={focusPort === null ? "暂无活动连接" : `端口 ${focusPort} 暂无活动连接`}
              description="当指纹浏览器开始通过端口访问网络时，连接会显示在这里。"
            />
          ) : (
            <div className={styles.groups}>
              {groups.map((group) => {
                const upload = group.items.reduce((sum, item) => sum + item.upload, 0);
                const download = group.items.reduce((sum, item) => sum + item.download, 0);
                return (
                  <section key={group.port} className={styles.group}>
                    <div className={styles.groupHead}>
                      <span className={styles.groupName}>{portName.get(group.port) ?? "端口"}</span>
                      <span className={`mono ${styles.groupAddr}`}>{proxyAddress(group.port)}</span>
                      <span className={styles.groupMeta}>{group.items.length} 条连接</span>
                      <span className={`mono ${styles.groupTraffic}`}>
                        <Icon name="arrowUp" size="sm" /> {formatBytes(upload)} · <Icon name="arrowDown" size="sm" /> {formatBytes(download)}
                      </span>
                    </div>
                    <table className={styles.connTable}>
                      <thead>
                        <tr>
                          <th>目标</th>
                          <th>进程</th>
                          <th>规则</th>
                          <th>速度</th>
                          <th>累计</th>
                        </tr>
                      </thead>
                      <tbody>
                        {group.items.map((conn) => (
                          <tr key={conn.id}>
                            <td className={styles.host} title={conn.destination}>{conn.host || conn.destination || "—"}</td>
                            <td className={styles.process}>{conn.process || "—"}</td>
                            <td className={styles.rule}>{conn.rule || conn.chain || "—"}</td>
                            <td className={`mono ${styles.speedCell}`}>
                              <span><Icon name="arrowUp" size="sm" /> {formatSpeed(conn.upload_speed)}</span>
                              <span><Icon name="arrowDown" size="sm" /> {formatSpeed(conn.download_speed)}</span>
                            </td>
                            <td className={`mono ${styles.speedCell}`}>
                              <span><Icon name="arrowUp" size="sm" /> {formatBytes(conn.upload)}</span>
                              <span><Icon name="arrowDown" size="sm" /> {formatBytes(conn.download)}</span>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </section>
                );
              })}
            </div>
          )}
        </div>
      ) : (
        <div className={styles.consoleWrap}>
          <div
            ref={logsPanelRef}
            id="activity-panel-logs"
            className={`${common.scroll} ${styles.console}`}
            role="tabpanel"
            aria-labelledby="activity-tab-logs"
            onScroll={handleLogsScroll}
          >
            {logs.length === 0 ? (
              <EmptyState icon={<Icon name="inbox" size="lg" />} title="暂无活动记录" description="启动核心后，运行日志会显示在这里。" />
            ) : (
              <ul className={styles.list}>
                {logs.map((entry, index) => (
                  <li key={`${entry.timestamp}-${index}`} className={styles.item}>
                    <span className={`mono ${styles.time}`}>{entry.timestamp}</span>
                    <span className={`${styles.level} ${levelClass[entry.level] ?? styles.info}`}>{entry.level}</span>
                    <span className={styles.message}>{entry.message}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
          {logs.length > 0 ? (
            <button
              type="button"
              className={`${styles.scrollToBottom}${stickToBottom ? ` ${styles.scrollToBottomFollowing}` : ""}`}
              onClick={jumpLogsToBottom}
              title={stickToBottom ? "正在跟随最新日志" : "滚动到底部"}
              aria-label={stickToBottom ? "正在跟随最新日志" : "滚动到底部"}
            >
              <Icon name="arrowDown" size="lg" />
            </button>
          ) : null}
        </div>
      )}

      <ConfirmDialog
        open={clearConfirmOpen}
        busy={false}
        title="清空运行日志？"
        message="当前页面中的诊断记录将被清除，此操作无法撤销。"
        confirmLabel="清空日志"
        onClose={() => setClearConfirmOpen(false)}
        onConfirm={() => {
          app.clearLogs();
          setClearConfirmOpen(false);
        }}
      />
    </div>
  );
}
