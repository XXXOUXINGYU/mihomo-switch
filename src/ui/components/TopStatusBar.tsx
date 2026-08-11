import type { MouseEvent } from "react";

import { Icon } from "./ui/Icon";
import styles from "./TopStatusBar.module.css";

type Props = {
  running: boolean;
  mihomoExists: boolean;
  busy: boolean;
  runningPortCount: number;
  invalidCount: number;
  showWindowControls: boolean;
  maximized: boolean;
  onAddPort: () => void;
  onToggleCore: () => void;
  theme: "light" | "dark";
  onToggleTheme: () => void;
  onInvalidClick: () => void;
  onStartDrag: () => void;
  onMinimize: () => void;
  onToggleMaximize: () => void;
  onHideToTray: () => void;
};

export function TopStatusBar(props: Props) {
  const statusLabel = props.running ? "核心运行中" : props.mihomoExists ? "核心待命" : "未配置内核";

  const handleMouseDown = (event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0 || props.showWindowControls) {
      return;
    }
    if ((event.target as HTMLElement).closest("button")) {
      return;
    }
    props.onStartDrag();
  };

  return (
    <header
      className={styles.bar}
      data-tauri-drag-region={props.showWindowControls ? "" : undefined}
      onMouseDown={handleMouseDown}
    >
      <div
        className={styles.brand}
        onDoubleClick={props.showWindowControls ? props.onToggleMaximize : undefined}
      >
        <span className={styles.mark} aria-hidden="true">M</span>
        <span className={styles.wordmark}>Mihomo Switch</span>
      </div>

      <div
        className={styles.status}
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        {props.busy ? (
          <>
            <span className="spinner" aria-hidden="true" />
            <span className={styles.statusText}>正在应用更改…</span>
          </>
        ) : (
          <>
            <span
              className={`dot ${props.running ? "dotLive" : props.mihomoExists ? "dotMuted" : "dotWarn"}`}
              aria-hidden="true"
            />
            <span className={styles.statusText}>{statusLabel}</span>
          </>
        )}
      </div>

      <div className={styles.readouts}>
        <div className={styles.readout}>
          <span className={`mono ${styles.readoutValue}`}>{props.runningPortCount}</span>
          <span className={styles.readoutLabel}>运行端口</span>
        </div>
        {props.invalidCount > 0 ? (
          <button type="button" className={styles.invalidChip} onClick={props.onInvalidClick}>
            <Icon name="alert" size="sm" className={styles.invalidIcon} />
            <span className={`mono ${styles.invalidCount}`}>{props.invalidCount}</span>
            <span>个失效绑定</span>
          </button>
        ) : null}
      </div>

      <div className={styles.actions}>
        <button type="button" className={`btn btnSm btnPrimarySoft ${styles.actionBtn}`} onClick={props.onAddPort} disabled={props.busy}>
          <Icon name="plus" size="sm" />
          新增端口
        </button>
        {props.running ? (
          <button type="button" className={`btn btnSm ${styles.actionBtn}`} onClick={props.onToggleCore} disabled={props.busy}>
            停止核心
          </button>
        ) : (
          <button
            type="button"
            className={`btn btnSm ${styles.actionBtn}`}
            onClick={props.onToggleCore}
            disabled={props.busy || !props.mihomoExists}
            title={props.mihomoExists ? "启动 mihomo" : "请先在设置中配置 mihomo.exe 路径"}
          >
            启动核心
          </button>
        )}
        <button
          className="iconBtn"
          onClick={props.onToggleTheme}
          disabled={props.busy}
          title={props.theme === "light" ? "切换到深色模式" : "切换到浅色模式"}
          aria-label={props.theme === "light" ? "切换到深色模式" : "切换到浅色模式"}
        >
          <Icon name={props.theme === "light" ? "moon" : "sun"} size="md" />
        </button>
      </div>

      {props.showWindowControls ? (
        <div className={styles.windowControls}>
          <button className={styles.windowButton} type="button" onClick={props.onMinimize} aria-label="最小化" title="最小化">
            <Icon name="minimize" size="md" />
          </button>
          <button
            className={styles.windowButton}
            type="button"
            onClick={props.onToggleMaximize}
            aria-label={props.maximized ? "还原" : "最大化"}
            title={props.maximized ? "还原" : "最大化"}
          >
            <Icon name={props.maximized ? "restore" : "maximize"} size="md" />
          </button>
          <button className={styles.windowButtonClose} type="button" onClick={props.onHideToTray} aria-label="隐藏到托盘" title="隐藏到托盘">
            <Icon name="close" size="md" />
          </button>
        </div>
      ) : null}
    </header>
  );
}
