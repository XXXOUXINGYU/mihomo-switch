import type { Toast } from "../../hooks/useToast";
import { Icon, type IconName } from "./Icon";
import styles from "./ToastStack.module.css";

type Props = {
  toasts: Toast[];
  onDismiss: (id: number) => void;
};

const ICON: Record<Toast["kind"], IconName> = {
  success: "check",
  info: "info",
  warn: "alert",
  error: "close",
};

export function ToastStack({ toasts, onDismiss }: Props) {
  if (toasts.length === 0) {
    return null;
  }
  return (
    <div className={styles.stack} role="status" aria-live="polite">
      {toasts.map((toast) => (
        <button
          key={toast.id}
          type="button"
          className={`${styles.toast} ${styles[toast.kind]}`}
          onClick={() => onDismiss(toast.id)}
        >
          <span className={styles.icon} aria-hidden="true">
            <Icon name={ICON[toast.kind]} size="sm" />
          </span>
          <span className={styles.message}>{toast.message}</span>
        </button>
      ))}
    </div>
  );
}
