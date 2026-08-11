import type { NodeTrafficPanel } from "../types";
import { useDialogFocus } from "../hooks/useDialogFocus";
import { useEscapeKey } from "../hooks/useEscapeKey";
import styles from "./NodeTrafficDialog.module.css";

type Props = {
  open: boolean;
  loading: boolean;
  error: string | null;
  panel: NodeTrafficPanel | null;
  onClose: () => void;
};

function formatBytes(value: number) {
  if (!Number.isFinite(value) || value <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let next = value;
  let index = 0;
  while (next >= 1024 && index < units.length - 1) {
    next /= 1024;
    index += 1;
  }
  const precision = next >= 100 || index === 0 ? 0 : next >= 10 ? 1 : 2;
  return `${next.toFixed(precision)} ${units[index]}`;
}

function formatRate(value: number) {
  return `${formatBytes(value)}/s`;
}

function formatLocalTime(value: string) {
  if (!value) {
    return "--";
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleTimeString("zh-CN", { hour12: false });
}

function compactChain(value: string) {
  if (!value) {
    return "--";
  }
  return value.replaceAll(" -> ", " => ");
}

export function NodeTrafficDialog(props: Props) {
  useEscapeKey(props.open, props.onClose);
  const dialogRef = useDialogFocus(props.open);
  if (!props.open) {
    return null;
  }

  const snapshot = props.panel?.snapshot ?? null;
  const connections = snapshot?.connections ?? [];
  const history = props.panel?.history ?? [];
  const mergedRows = [
    ...connections.map((item) => ({
      id: `live-${item.id}`,
      host: item.host || item.destination || item.id,
      chain: item.chains.join(" -> ") || item.process || "--",
      uploadSpeed: item.upload_speed,
      downloadSpeed: item.download_speed,
      upload: item.upload,
      download: item.download,
      time: item.start,
      timeLabel: "活动中",
      status: "活动",
      note: "",
    })),
    ...history.map((item) => ({
      id: `history-${item.id}`,
      host: item.host || item.destination || "--",
      chain: item.chain || item.process || "--",
      uploadSpeed: item.upload_speed,
      downloadSpeed: item.download_speed,
      upload: item.upload,
      download: item.download,
      time: item.time,
      timeLabel: formatLocalTime(item.time),
      status: "记录",
      note: item.note,
    })),
  ];
  const title = snapshot
    ? `${snapshot.sub_name} / ${snapshot.node_name}`
    : "节点流量分析";

  return (
    <div className={styles.backdrop} onClick={props.onClose}>
      <div
        ref={dialogRef}
        className={styles.dialog}
        role="dialog"
        aria-modal="true"
        aria-label="节点流量分析"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <div className={styles.header}>
          <div>
            <h2 className={styles.title}>节点流量分析</h2>
            <p className={styles.subtitle}>{title}</p>
          </div>
          <button className={styles.close} onClick={props.onClose}>
            关闭
          </button>
        </div>

        <div className={styles.stats}>
          <article className={styles.card}>
            <span className={styles.cardLabel}>监控累计</span>
            <strong className={styles.cardValue}>
              下行 {formatBytes(props.panel?.session_download ?? 0)}
            </strong>
            <span className={styles.cardHint}>上行 {formatBytes(props.panel?.session_upload ?? 0)}</span>
          </article>
          <article className={styles.card}>
            <span className={styles.cardLabel}>当前活动</span>
            <strong className={styles.cardValue}>{connections.length}</strong>
            <span className={styles.cardHint}>
              {snapshot?.local_port ? `本地端口 ${snapshot.local_port}` : "未启用"}
            </span>
          </article>
          <article className={styles.card}>
            <span className={styles.cardLabel}>记录总数</span>
            <strong className={styles.cardValue}>{props.panel?.total_records ?? 0}</strong>
            <span className={styles.cardHint}>
              最近窗口最多展示 200 条
            </span>
          </article>
        </div>

        <div className={styles.banner}>
          {props.loading ? "正在刷新流量记录..." : props.error || snapshot?.message || "暂无数据"}
        </div>

        <section className={styles.panel}>
          <div className={styles.panelHead}>
            <div>
              <h3 className={styles.panelTitle}>实时活动记录</h3>
              <p className={styles.panelHint}>活动连接和实时日志已合并到同一张表，后台持续保留最近 200 条记录。</p>
            </div>
            <span className={styles.panelMeta}>
              {snapshot
                ? `活动 ${connections.length} · 累计 ${props.panel?.total_records ?? 0} · 展示 ${history.length}/200 · 采样 ${formatLocalTime(snapshot.sampled_at)}`
                : "--"}
            </span>
          </div>

          <div className={styles.tableWrap}>
            {mergedRows.length ? (
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th>主机</th>
                    <th>链路</th>
                    <th>上行速度</th>
                    <th>下行速度</th>
                    <th>上行流量</th>
                    <th>下行流量</th>
                    <th>记录时间</th>
                  </tr>
                </thead>
                <tbody>
                  {mergedRows.map((item) => (
                    <tr key={item.id}>
                      <td>
                        <div className={styles.hostCell}>
                          <span className={styles.statusBadge}>{item.status}</span>
                          <div className={styles.hostBlock}>
                            <strong className={styles.hostName}>{item.host}</strong>
                            {"note" in item && item.note ? (
                              <span className={styles.hostMeta}>{item.note}</span>
                            ) : null}
                          </div>
                        </div>
                      </td>
                      <td title={item.chain}>{compactChain(item.chain)}</td>
                      <td>{formatRate(item.uploadSpeed)}</td>
                      <td>{formatRate(item.downloadSpeed)}</td>
                      <td>{formatBytes(item.upload)}</td>
                      <td>{formatBytes(item.download)}</td>
                      <td>{item.timeLabel}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div className={styles.emptyState}>当前没有活动连接，也还没有采样到实时记录。</div>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
