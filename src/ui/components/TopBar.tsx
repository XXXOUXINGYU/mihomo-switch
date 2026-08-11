import styles from "./TopBar.module.css";

type Props = {
  currentSubName: string;
  selectedCount: number;
  activeSubCount: number;
  running: boolean;
  mihomoExists: boolean;
  busy: boolean;
  showWindowControls: boolean;
  maximized: boolean;
  canManageCurrent: boolean;
  onAdd: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onImport: () => void;
  onSettings: () => void;
  onStart: () => void;
  onStop: () => void;
  onStartDrag: () => void;
  onMinimize: () => void;
  onToggleMaximize: () => void;
  onHideToTray: () => void;
};

export function TopBar(props: Props) {
  const startDrag = (event: React.MouseEvent) => {
    if (event.button !== 0) {
      return;
    }
    props.onStartDrag();
  };

  const statusLabel = props.running
    ? "运行中"
    : props.mihomoExists
      ? "待命"
      : "未配置内核";
  const statusClass = props.running
    ? styles.statusLive
    : props.mihomoExists
      ? styles.statusIdle
      : styles.statusOffline;

  return (
    <header className={styles.bar} onMouseDown={startDrag}>
      <div className={styles.dragEdge} aria-hidden="true" />

      <div className={styles.brand} onDoubleClick={props.showWindowControls ? props.onToggleMaximize : undefined}>
        <span className={styles.mark} aria-hidden="true" />
        <span className={styles.wordmark}>
          MIHOMO<span className={styles.wordmarkAccent}>SWITCH</span>
        </span>
      </div>

      <div className={statusClass}>
        <span className={styles.statusDot} aria-hidden="true" />
        <span className={styles.statusText}>{statusLabel}</span>
      </div>

      <div className={styles.readouts} onMouseDown={(event) => event.stopPropagation()}>
        <div className={styles.readout}>
          <span className={styles.readoutValue}>{props.selectedCount}</span>
          <span className={styles.readoutLabel}>启用节点</span>
        </div>
        <div className={styles.readoutDivider} aria-hidden="true" />
        <div className={styles.readout}>
          <span className={styles.readoutValue}>{props.activeSubCount}</span>
          <span className={styles.readoutLabel}>活动订阅</span>
        </div>
        <div className={styles.readoutDivider} aria-hidden="true" />
        <div className={styles.readoutWide} title={props.currentSubName}>
          <span className={styles.readoutValueText}>{props.currentSubName}</span>
          <span className={styles.readoutLabel}>当前订阅</span>
        </div>
      </div>

      <div className={styles.actions} onMouseDown={(event) => event.stopPropagation()}>
        <button className={styles.ghost} onClick={props.onSettings} disabled={props.busy}>
          设置
        </button>
        <button className={styles.ghost} onClick={props.onImport} disabled={props.busy || !props.canManageCurrent}>
          导入
        </button>
        <button className={styles.ghost} onClick={props.onEdit} disabled={props.busy || !props.canManageCurrent}>
          编辑
        </button>
        <button className={styles.ghostDanger} onClick={props.onDelete} disabled={props.busy || !props.canManageCurrent}>
          删除
        </button>
        <button className={styles.addButton} onClick={props.onAdd} disabled={props.busy}>
          <span className={styles.addIcon}>+</span>
          添加
        </button>
        {props.running ? (
          <button className={styles.stop} onClick={props.onStop} disabled={props.busy}>
            停止
          </button>
        ) : (
          <button
            className={styles.start}
            onClick={props.onStart}
            disabled={props.busy || !props.mihomoExists}
            title={props.mihomoExists ? "启动 mihomo" : "请先在设置中配置 mihomo.exe 路径"}
          >
            启动
          </button>
        )}
      </div>

      {props.showWindowControls ? (
        <div className={styles.windowControls} onMouseDown={(event) => event.stopPropagation()}>
          <button
            className={styles.windowButton}
            type="button"
            onClick={props.onMinimize}
            aria-label="最小化窗口"
            title="最小化"
          >
            <span className={styles.windowGlyph}>–</span>
          </button>
          <button
            className={styles.windowButton}
            type="button"
            onClick={props.onToggleMaximize}
            aria-label={props.maximized ? "还原窗口" : "最大化窗口"}
            title={props.maximized ? "还原" : "最大化"}
          >
            <span className={styles.windowGlyph}>{props.maximized ? "❐" : "□"}</span>
          </button>
          <button
            className={styles.windowButtonDanger}
            type="button"
            onClick={props.onHideToTray}
            aria-label="隐藏到系统托盘"
            title="隐藏到托盘"
          >
            <span className={styles.windowGlyph}>×</span>
          </button>
        </div>
      ) : null}
    </header>
  );
}
