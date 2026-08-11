import type { LogEntry } from "../types";
import styles from "./RightPanel.module.css";

type Props = {
  logs: LogEntry[];
  onClear: () => void;
};

function lineClass(level: string) {
  switch (level) {
    case "error":
      return styles.lineError;
    case "warn":
      return styles.lineWarn;
    case "info":
      return styles.lineInfo;
    default:
      return styles.lineMuted;
  }
}

export function RightPanel(props: Props) {
  return (
    <section className={styles.wrap}>
      <div className={styles.head}>
        <h2 className={styles.title}>
          控制台
          <span className={styles.count}>{props.logs.length}</span>
        </h2>
        <button className={styles.clearButton} onClick={props.onClear}>
          清空
        </button>
      </div>
      <div className={styles.log}>
        {props.logs.length === 0 ? (
          <div className={styles.emptyLog}>
            日志会在导入订阅、生成配置和启动内核时出现在这里。
          </div>
        ) : (
          props.logs.map((entry, index) => (
            <div key={`${entry.timestamp}-${index}`} className={`${styles.line} ${lineClass(entry.level)}`}>
              <span className={styles.lineTime}>{entry.timestamp}</span>
              <span className={styles.lineGutter} aria-hidden="true" />
              <span className={styles.lineText}>{entry.message}</span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
