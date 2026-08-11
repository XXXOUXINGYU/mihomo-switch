import type { PortSlotView } from "../types";
import { useDialogFocus } from "../hooks/useDialogFocus";
import { useEscapeKey } from "../hooks/useEscapeKey";
import { proxyAddress } from "../lib/format";
import styles from "./SlotPickerDialog.module.css";

type Props = {
  open: boolean;
  busy: boolean;
  nodeName: string;
  slots: PortSlotView[];
  onCancel: () => void;
  onPick: (slotId: string) => void;
};

export function SlotPickerDialog(props: Props) {
  useEscapeKey(props.open && !props.busy, props.onCancel);
  const dialogRef = useDialogFocus(props.open);
  if (!props.open) {
    return null;
  }

  return (
    <div className="overlay" onClick={props.busy ? undefined : props.onCancel}>
      <div
        ref={dialogRef}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label="绑定到现有端口"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialogHeader">
          <h2 className="dialogTitle">绑定到现有端口</h2>
          <p className="dialogSubtitle">
            将节点「{props.nodeName}」绑定到下面某个端口，端口号保持不变，原绑定会被替换。
          </p>
        </div>

        <div className="dialogBody">
          {props.slots.length === 0 ? (
            <p className={styles.empty}>还没有端口，请先在端口管理中创建端口。</p>
          ) : (
            <ul className={styles.list}>
              {props.slots.map((slot) => (
                <li key={slot.id}>
                  <button
                    type="button"
                    className={styles.row}
                    disabled={props.busy}
                    onClick={() => props.onPick(slot.id)}
                  >
                    <div className={styles.main}>
                      <span className={styles.name}>{slot.name}</span>
                      <span className={`mono ${styles.address}`}>{proxyAddress(slot.local_port)}</span>
                    </div>
                    <span className={styles.current}>
                      {slot.binding && slot.state === "valid"
                        ? `当前：${slot.binding.node_name}`
                        : slot.state === "unbound"
                          ? "当前未绑定"
                          : "当前已失效"}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="dialogFooter">
          <button className="btn" onClick={props.onCancel} disabled={props.busy}>取消</button>
        </div>
      </div>
    </div>
  );
}
