import { useEffect, useState } from "react";

import type { PortSlotView, SubscriptionRecord } from "../types";
import type { LatencyMap } from "../hooks/useAppData";
import { useDialogFocus } from "../hooks/useDialogFocus";
import { useEscapeKey } from "../hooks/useEscapeKey";
import { NodePicker, type NodeChoice } from "./NodePicker";
import { proxyAddress } from "../lib/format";
import { detectRegion } from "../lib/region";
import styles from "./BatchRebindDialog.module.css";

export type BatchAssignment = { slotId: string; subId: string; nodeIndex: number };

type Props = {
  open: boolean;
  busy: boolean;
  slots: PortSlotView[];
  subscriptions: SubscriptionRecord[];
  latencyMap: LatencyMap;
  onTest: (subId: string, nodeIndex: number) => void;
  onCancel: () => void;
  onApply: (assignments: BatchAssignment[]) => void;
};

type PickerTarget = { mode: "single"; slotId: string } | { mode: "all" } | null;

export function BatchRebindDialog(props: Props) {
  const [choices, setChoices] = useState<Record<string, NodeChoice>>({});
  const [picker, setPicker] = useState<PickerTarget>(null);

  useEscapeKey(props.open && picker === null && !props.busy, props.onCancel);
  const dialogRef = useDialogFocus(props.open && picker === null);

  useEffect(() => {
    if (props.open) {
      setChoices({});
      setPicker(null);
    }
  }, [props.open]);

  if (!props.open) {
    return null;
  }

  const labelFor = (choice: NodeChoice | undefined) => {
    if (!choice) {
      return null;
    }
    const sub = props.subscriptions.find((item) => item.id === choice.subId);
    const node = sub?.nodes[choice.nodeIndex];
    if (!sub || !node) {
      return null;
    }
    return { name: node.name, flag: detectRegion(node.name).flag, subName: sub.name };
  };

  const chosenCount = Object.keys(choices).length;

  const handleConfirm = (choice: NodeChoice) => {
    if (picker?.mode === "all") {
      const next: Record<string, NodeChoice> = {};
      for (const slot of props.slots) {
        next[slot.id] = choice;
      }
      setChoices(next);
    } else if (picker?.mode === "single") {
      setChoices((prev) => ({ ...prev, [picker.slotId]: choice }));
    }
    setPicker(null);
  };

  const pickerPortName =
    picker?.mode === "single"
      ? props.slots.find((slot) => slot.id === picker.slotId)?.name ?? "端口"
      : "全部失效端口";
  const pickerLocalPort =
    picker?.mode === "single"
      ? props.slots.find((slot) => slot.id === picker.slotId)?.local_port ?? 0
      : 0;
  const pickerCurrent =
    picker?.mode === "single" ? choices[picker.slotId] ?? null : null;

  return (
    <div className="overlay" onClick={props.busy ? undefined : props.onCancel}>
      <div
        ref={dialogRef}
        className="dialog dialogBatch"
        role="dialog"
        aria-modal="true"
        aria-label="批量重新绑定"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialogHeader">
          <h2 className="dialogTitle">批量重新绑定</h2>
          <p className="dialogSubtitle">
            为 {props.slots.length} 个失效或未绑定的端口重新选择节点。端口号保持不变。
          </p>
        </div>

        <div className="dialogBody">
          <div className={styles.bulkRow}>
            <button type="button" className="btn btnSm" onClick={() => setPicker({ mode: "all" })}>
              全部绑定到同一节点
            </button>
            {chosenCount > 0 ? (
              <button type="button" className="btn btnSm btnGhost" onClick={() => setChoices({})}>
                清空选择
              </button>
            ) : null}
          </div>

          <ul className={styles.list}>
            {props.slots.map((slot) => {
              const label = labelFor(choices[slot.id]);
              return (
                <li key={slot.id} className={styles.item}>
                  <div className={styles.itemMain}>
                    <span className={styles.itemName}>{slot.name}</span>
                    <span className={`mono ${styles.itemAddress}`}>{proxyAddress(slot.local_port)}</span>
                  </div>
                  <div className={styles.itemChoice}>
                    {label ? (
                      <span className={styles.chosen}>
                        <span aria-hidden="true">{label.flag}</span>
                        <span className={styles.chosenName}>{label.name}</span>
                      </span>
                    ) : (
                      <span className={styles.unchosen}>未选择</span>
                    )}
                    <button type="button" className="btn btnSm" onClick={() => setPicker({ mode: "single", slotId: slot.id })}>
                      {label ? "更换" : "选择"}
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        </div>

        <div className="dialogFooter">
          <button className="btn" onClick={props.onCancel} disabled={props.busy}>取消</button>
          <button
            className="btn btnPrimary"
            disabled={props.busy || chosenCount === 0}
            onClick={() =>
              props.onApply(
                Object.entries(choices).map(([slotId, choice]) => ({
                  slotId,
                  subId: choice.subId,
                  nodeIndex: choice.nodeIndex,
                })),
              )
            }
          >
            应用绑定（{chosenCount}）
          </button>
        </div>
      </div>

      <NodePicker
        open={picker !== null}
        title={picker?.mode === "all" ? "为全部失效端口选择节点" : "选择节点"}
        portName={pickerPortName}
        localPort={pickerLocalPort}
        subscriptions={props.subscriptions}
        currentNodeName={null}
        current={pickerCurrent}
        latencyMap={props.latencyMap}
        onTest={props.onTest}
        onCancel={() => setPicker(null)}
        onConfirm={handleConfirm}
      />
    </div>
  );
}
