import { useEffect, useState, type KeyboardEvent } from "react";

import type { UpsertSubscriptionInput } from "../types";
import { useDialogFocus } from "../hooks/useDialogFocus";
import { useEscapeKey } from "../hooks/useEscapeKey";
import styles from "./SubscriptionDialog.module.css";

type FormValue = {
  name: string;
  url: string;
  manual: boolean;
  content: string;
};

type Props = {
  open: boolean;
  mode: "create" | "edit";
  busy: boolean;
  initialValue: FormValue | null;
  onClose: () => void;
  onSubmit: (input: UpsertSubscriptionInput) => void;
};

export function SubscriptionDialog(props: Props) {
  const [manual, setManual] = useState(false);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [content, setContent] = useState("");

  useEscapeKey(props.open && !props.busy, props.onClose);
  const dialogRef = useDialogFocus(props.open);

  useEffect(() => {
    if (!props.open) return;
    setManual(props.initialValue?.manual ?? false);
    setName(props.initialValue?.name ?? "");
    setUrl(props.initialValue?.url ?? "");
    setContent(props.initialValue?.content ?? "");
  }, [props.initialValue, props.open]);

  if (!props.open) return null;

  const submitDisabled = props.busy || (!manual && !url.trim()) || (manual && !content.trim());
  const selectSourceByKey = (event: KeyboardEvent<HTMLButtonElement>, currentManual: boolean) => {
    let nextManual = currentManual;
    if (event.key === "ArrowRight" || event.key === "ArrowLeft") nextManual = !currentManual;
    else if (event.key === "Home") nextManual = false;
    else if (event.key === "End") nextManual = true;
    else return;
    event.preventDefault();
    setManual(nextManual);
    event.currentTarget.parentElement
      ?.querySelector<HTMLButtonElement>(`#subscription-tab-${nextManual ? "manual" : "url"}`)
      ?.focus();
  };

  return (
    <div className="overlay" onClick={props.busy ? undefined : props.onClose}>
      <div
        ref={dialogRef}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={props.mode === "edit" ? "编辑订阅" : "新增订阅"}
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialogHeader">
          <h2 className="dialogTitle">{props.mode === "edit" ? "编辑订阅" : "新增订阅"}</h2>
          <p className="dialogSubtitle">
            {props.mode === "edit"
              ? "调整名称、来源或手动内容。手动订阅会在保存时立即解析节点。"
              : "支持 URL 订阅和手动粘贴两种来源，手动内容会在保存后立即生效。"}
          </p>
        </div>

        <div className="dialogBody">
          <div className={styles.tabs} role="tablist" aria-label="订阅来源">
            <button
              id="subscription-tab-url"
              type="button"
              role="tab"
              aria-selected={!manual}
              aria-controls="subscription-panel-url"
              tabIndex={manual ? -1 : 0}
              className={manual ? styles.tab : styles.tabActive}
              onClick={() => setManual(false)}
              onKeyDown={(event) => selectSourceByKey(event, false)}
            >
              URL 订阅
            </button>
            <button
              id="subscription-tab-manual"
              type="button"
              role="tab"
              aria-selected={manual}
              aria-controls="subscription-panel-manual"
              tabIndex={manual ? 0 : -1}
              className={manual ? styles.tabActive : styles.tab}
              onClick={() => setManual(true)}
              onKeyDown={(event) => selectSourceByKey(event, true)}
            >
              手动粘贴
            </button>
          </div>

          <label className="field">
            <span className="fieldLabel">名称</span>
            <input className="input" value={name} onChange={(event) => setName(event.target.value)} />
          </label>

          {manual ? (
            <label
              id="subscription-panel-manual"
              className="field"
              role="tabpanel"
              aria-labelledby="subscription-tab-manual"
            >
              <span className="fieldLabel">节点内容</span>
              <textarea className="textarea" value={content} onChange={(event) => setContent(event.target.value)} />
            </label>
          ) : (
            <label
              id="subscription-panel-url"
              className="field"
              role="tabpanel"
              aria-labelledby="subscription-tab-url"
            >
              <span className="fieldLabel">订阅 URL</span>
              <input className="input" value={url} onChange={(event) => setUrl(event.target.value)} />
            </label>
          )}
        </div>

        <div className="dialogFooter">
          <button className="btn" onClick={props.onClose} disabled={props.busy}>取消</button>
          <button
            className="btn btnPrimary"
            disabled={submitDisabled}
            onClick={() =>
              props.onSubmit({
                name: name.trim(),
                manual,
                url: url.trim(),
                content: content.trim(),
              })
            }
          >
            {props.mode === "edit" ? "保存修改" : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}
