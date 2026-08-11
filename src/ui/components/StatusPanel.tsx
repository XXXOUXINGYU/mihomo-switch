import { formatBytes } from "../lib/format";
import styles from "./StatusPanel.module.css";

type Props = {
  totalUpload: number;
  totalDownload: number;
  totalConnections: number;
};

export function StatusPanel(props: Props) {
  return (
    <section className={styles.panel} aria-label="流量概览">
      <span className={styles.heading}>流量概览</span>
      <div className={styles.metrics}>
        <Metric label="上行" value={formatBytes(props.totalUpload)} />
        <Metric label="下行" value={formatBytes(props.totalDownload)} />
        <Metric label="活动连接" value={props.totalConnections} />
      </div>
    </section>
  );
}

function Metric(props: { label: string; value: string | number }) {
  return (
    <div className={styles.metric}>
      <span className={styles.metricLabel}>{props.label}</span>
      <span className={`mono ${styles.metricValue}`}>{props.value}</span>
    </div>
  );
}
