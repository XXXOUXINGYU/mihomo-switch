import { useEffect, useMemo, useState } from "react";

import type { PortSlotInput, PortValidation, SubscriptionRecord } from "../types";
import type { LatencyMap } from "../hooks/useAppData";
import { useDialogFocus } from "../hooks/useDialogFocus";
import { useEscapeKey } from "../hooks/useEscapeKey";
import { NodePicker, type NodeChoice } from "./NodePicker";
import { detectRegion } from "../lib/region";
import styles from "./PortFormDialog.module.css";

export type PortFormInitial = {
  id: string;
  name: string;
  note: string;
  localPort: number;
  enabled: boolean;
  binding: NodeChoice | null;
};

type Props = {
  open: boolean;
  mode: "create" | "edit";
  initial: PortFormInitial | null;
  subscriptions: SubscriptionRecord[];
  latencyMap: LatencyMap;
  busy: boolean;
  suggestedPort: number;
  onTest: (subId: string, nodeIndex: number) => void;
  validatePort: (port: number, ignoreSlotId: string | null) => Promise<PortValidation>;
  onClose: () => void;
  onSubmit: (input: PortSlotInput) => void;
};

export function PortFormDialog(props: Props) {
  const [name, setName] = useState("");
  const [portText, setPortText] = useState("");
  const [note, setNote] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [binding, setBinding] = useState<NodeChoice | null>(null);
  const [validation, setValidation] = useState<PortValidation | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);

  useEscapeKey(props.open && !pickerOpen && !props.busy, props.onClose);
  const dialogRef = useDialogFocus(props.open && !pickerOpen);

  useEffect(() => {
    if (!props.open) {
      return;
    }
    setName(props.initial?.name ?? "");
    setPortText(String(props.initial?.localPort ?? props.suggestedPort));
    setNote(props.initial?.note ?? "");
    setEnabled(props.initial?.enabled ?? true);
    setBinding(props.initial?.binding ?? null);
    setValidation(null);
    setPickerOpen(false);
  }, [props.open, props.initial, props.suggestedPort]);

  const port = Number.parseInt(portText, 10);
  const ignoreId = props.mode === "edit" ? props.initial?.id ?? null : null;

  useEffect(() => {
    if (!props.open) {
      return;
    }
    if (!Number.isFinite(port)) {
      setValidation({ status: "invalid", message: "请输入端口号" });
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void props.validatePort(port, ignoreId).then((result) => {
        if (!cancelled) {
          setValidation(result);
        }
      });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [port, ignoreId, props.open]);

  const bindingLabel = useMemo(() => {
    if (!binding) {
      return null;
    }
    const sub = props.subscriptions.find((item) => item.id === binding.subId);
    const node = sub?.nodes[binding.nodeIndex];
    if (!sub || !node) {
      return null;
    }
    const region = detectRegion(node.name);
    return { subName: sub.name, nodeName: node.name, flag: region.flag };
  }, [binding, props.subscriptions]);

  if (!props.open) {
    return null;
  }

  const portInvalid = validation !== null && validation.status !== "ok";
  const submitDisabled = props.busy || !name.trim() || !Number.isFinite(port) || portInvalid;

  const currentNodeName = bindingLabel?.nodeName ?? null;
  const portHintId = "port-validation-hint";

  return (
    <div className="overlay" onClick={props.busy ? undefined : props.onClose}>
      <div
        ref={dialogRef}
        className={`dialog ${styles.dialog}`}
        role="dialog"
        aria-modal="true"
        aria-label={props.mode === "edit" ? "编辑端口" : "新增端口"}
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialogHeader">
          <h2 className="dialogTitle">{props.mode === "edit" ? "编辑端口" : "新增端口"}</h2>
          <p className="dialogSubtitle">
            端口号是稳定的本地代理地址，指纹浏览器始终连接它。更换节点不会改变端口号。
          </p>
        </div>

        <div className="dialogBody">
          <label className="field">
            <span className="fieldLabel">端口名称</span>
            <input
              className="input"
              placeholder="例如：美区主力"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>

          <label className="field">
            <span className="fieldLabel">本地端口号</span>
            <input
              className={`input mono ${portInvalid ? "inputInvalid" : ""}`}
              inputMode="numeric"
              placeholder="1024 - 65535"
              value={portText}
              onChange={(event) => setPortText(event.target.value.replace(/[^0-9]/g, ""))}
              aria-invalid={portInvalid}
              aria-describedby={portHintId}
            />
            <span id={portHintId} role={portInvalid ? "alert" : "status"} className={portInvalid ? styles.errorHint : "fieldHint"}>
              {validation
                ? validation.status === "ok"
                  ? `本地代理地址：127.0.0.1:${port}`
                  : validation.message
                : "正在校验端口…"}
            </span>
          </label>

          <label className="field">
            <span className="fieldLabel">备注</span>
            <input
              className="input"
              placeholder="例如：AdsPower 窗口 12"
              value={note}
              onChange={(event) => setNote(event.target.value)}
            />
          </label>

          <div className="field">
            <span className="fieldLabel">绑定节点</span>
            <div className={styles.bindingRow}>
              {bindingLabel ? (
                <span className={styles.bindingNode}>
                  <span aria-hidden="true">{bindingLabel.flag}</span>
                  <span className={styles.bindingName}>{bindingLabel.nodeName}</span>
                  <span className={styles.bindingSub}>{bindingLabel.subName}</span>
                </span>
              ) : (
                <span className={styles.bindingEmpty}>暂未绑定，可稍后再选择</span>
              )}
              <div className={styles.bindingActions}>
                {binding ? (
                  <button type="button" className="btn btnSm btnGhost" onClick={() => setBinding(null)}>
                    清除
                  </button>
                ) : null}
                <button type="button" className="btn btnSm" onClick={() => setPickerOpen(true)}>
                  {binding ? "更换节点" : "选择节点"}
                </button>
              </div>
            </div>
          </div>

          <label className={styles.enableRow}>
            <button
              type="button"
              className={enabled ? "switch switchOn" : "switch"}
              role="switch"
              aria-checked={enabled}
              aria-label="启用端口"
              onClick={() => setEnabled((prev) => !prev)}
            />
            <span>创建后立即启用此端口</span>
          </label>
        </div>

        <div className="dialogFooter">
          <button className="btn" onClick={props.onClose} disabled={props.busy}>取消</button>
          <button
            className="btn btnPrimary"
            disabled={submitDisabled}
            onClick={() =>
              props.onSubmit({
                name: name.trim(),
                local_port: port,
                note: note.trim(),
                enabled,
                binding: binding ? { sub_id: binding.subId, node_index: binding.nodeIndex } : null,
              })
            }
          >
            {props.mode === "edit" ? "保存修改" : "创建端口"}
          </button>
        </div>
      </div>

      <NodePicker
        open={pickerOpen}
        portName={name.trim() || "新端口"}
        localPort={Number.isFinite(port) ? port : 0}
        subscriptions={props.subscriptions}
        currentNodeName={currentNodeName}
        current={binding}
        latencyMap={props.latencyMap}
        onTest={props.onTest}
        onCancel={() => setPickerOpen(false)}
        onConfirm={(choice) => {
          setBinding(choice);
          setPickerOpen(false);
        }}
      />
    </div>
  );
}
