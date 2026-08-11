import { useEffect, useState } from "react";

type Props = {
  open: boolean;
  busy: boolean;
  initialEnabled: boolean;
  initialUrl: string;
  initialMihomoPath: string;
  onClose: () => void;
  onSubmit: (enabled: boolean, url: string, mihomoPath: string) => void;
};

export function SettingsDialog(props: Props) {
  const [enabled, setEnabled] = useState(false);
  const [url, setUrl] = useState("");
  const [mihomoPath, setMihomoPath] = useState("");

  useEffect(() => {
    if (!props.open) {
      return;
    }
    setEnabled(props.initialEnabled);
    setUrl(props.initialUrl);
    setMihomoPath(props.initialMihomoPath);
  }, [props.initialEnabled, props.initialMihomoPath, props.initialUrl, props.open]);

  if (!props.open) {
    return null;
  }

  return (
    <div className="overlay" onClick={props.onClose}>
      <div className="dialog" onClick={(event) => event.stopPropagation()}>
        <div className="dialogHeader">
          <h2 className="dialogTitle">设置</h2>
          <p className="dialogSubtitle">
            控制客户端访问外部网络时是否走本地代理。mihomo 内核由用户自行下载，并在这里配置 exe 路径。
          </p>
        </div>

        <div className="dialogBody">
          <label style={{ display: "flex", alignItems: "center", gap: 10, cursor: "pointer" }}>
            <button
              type="button"
              className={enabled ? "switch switchOn" : "switch"}
              role="switch"
              aria-checked={enabled}
              aria-label="启用本地代理"
              onClick={() => setEnabled((prev) => !prev)}
            />
            <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>启用本地代理（应用自身访问网络时使用）</span>
          </label>

          <label className="field">
            <span className="fieldLabel">代理地址</span>
            <input
              className="input mono"
              placeholder="例如: http://127.0.0.1:20122"
              value={url}
              onChange={(event) => setUrl(event.target.value)}
            />
          </label>

          <label className="field">
            <span className="fieldLabel">mihomo.exe 文件路径</span>
            <input
              className="input mono"
              placeholder="例如: C:\\Users\\你的用户名\\.mihomo_switch\\mihomo.exe"
              value={mihomoPath}
              onChange={(event) => setMihomoPath(event.target.value)}
            />
          </label>
        </div>

        <div className="dialogFooter">
          <button className="btn" onClick={props.onClose}>取消</button>
          <button
            className="btn btnPrimary"
            disabled={props.busy}
            onClick={() => props.onSubmit(enabled, url.trim(), mihomoPath.trim())}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
