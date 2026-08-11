import type { PortSlotView } from "../types";
import { useDialogFocus } from "../hooks/useDialogFocus";
import { useEscapeKey } from "../hooks/useEscapeKey";
import { proxyAddress } from "../lib/format";
import styles from "./StartReviewDialog.module.css";

type Props = {
  open: boolean;
  busy: boolean;
  runnable: PortSlotView[];
  blocked: PortSlotView[];
  onCancel: () => void;
  onConfirm: () => void;
};

function reasonOf(slot: PortSlotView) {
  if (slot.state === "unbound") {
    return "未绑定节点";
  }
  return slot.invalid_reason ?? "节点不可用";
}

export function StartReviewDialog(props: Props) {
  useEscapeKey(props.open && !props.busy, props.onCancel);
  const dialogRef = useDialogFocus(props.open);
  if (!props.open) {
    return null;
  }
  const hasRunnable = props.runnable.length > 0;

  return (
    <div className="overlay" onClick={props.busy ? undefined : props.onCancel}>
      <div
        ref={dialogRef}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label="启动前检查"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialogHeader">
          <h2 className="dialogTitle">启动前检查</h2>
          <p className="dialogSubtitle">
            以下 {props.blocked.length} 个已启用端口无法启动，将被跳过。请确认是否继续。
          </p>
        </div>

        <div className="dialogBody">
          <div className={styles.summary}>
            <span className={`tag ${hasRunnable ? "tagGood" : "tagMuted"}`}>
              可启动 {props.runnable.length}
            </span>
            <span className="tag tagWarn">跳过 {props.blocked.length}</span>
          </div>

          <ul className={styles.list}>
            {props.blocked.map((slot) => (
              <li key={slot.id} className={styles.item}>
                <div className={styles.itemMain}>
                  <span className={styles.itemName}>{slot.name}</span>
                  <span className={`mono ${styles.itemAddress}`}>{proxyAddress(slot.local_port)}</span>
                </div>
                <span className={`tag tagWarn ${styles.reason}`}>{reasonOf(slot)}</span>
              </li>
            ))}
          </ul>

          {!hasRunnable ? (
            <p className={styles.noRunnable}>当前没有可启动的端口，请先为端口绑定有效节点。</p>
          ) : null}
        </div>

        <div className="dialogFooter">
          <button className="btn" onClick={props.onCancel} disabled={props.busy}>取消</button>
          <button className="btn btnPrimary" disabled={props.busy || !hasRunnable} onClick={props.onConfirm}>
            启动可用端口
          </button>
        </div>
      </div>
    </div>
  );
}
