import type { ReactNode } from "react";

import { useDialogFocus } from "../hooks/useDialogFocus";
import { useEscapeKey } from "../hooks/useEscapeKey";

type Props = {
  open: boolean;
  busy: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  tone?: "danger" | "primary";
  children?: ReactNode;
  onClose: () => void;
  onConfirm: () => void;
};

export function ConfirmDialog(props: Props) {
  useEscapeKey(props.open && !props.busy, props.onClose);
  const dialogRef = useDialogFocus(props.open);
  if (!props.open) {
    return null;
  }
  const tone = props.tone ?? "danger";

  return (
    <div className="overlay" onClick={props.busy ? undefined : props.onClose}>
      <div
        ref={dialogRef}
        className="dialog dialogConfirm"
        tabIndex={-1}
        role="alertdialog"
        aria-modal="true"
        aria-label={props.title}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialogHeader">
          <h2 className="dialogTitle">{props.title}</h2>
          <p className="dialogSubtitle">{props.message}</p>
        </div>
        {props.children ? <div className="dialogBody">{props.children}</div> : null}
        <div className="dialogFooter">
          <button className="btn" onClick={props.onClose} disabled={props.busy}>取消</button>
          <button
            className={tone === "danger" ? "btn btnDangerSolid" : "btn btnPrimary"}
            onClick={props.onConfirm}
            disabled={props.busy}
          >
            {props.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
