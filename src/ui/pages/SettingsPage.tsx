import { useEffect, useState } from "react";

import type { AppData } from "../hooks/useAppData";
import common from "./common.module.css";
import styles from "./SettingsPage.module.css";

type Props = {
  app: AppData;
};

function localProxyError(enabled: boolean, value: string) {
  if (!enabled || !value.trim()) return "";
  try {
    const parsed = new URL(value.trim());
    return (parsed.protocol === "http:" || parsed.protocol === "https:") && parsed.hostname
      ? ""
      : "请输入完整的 http:// 或 https:// 代理地址。";
  } catch {
    return "请输入完整的 http:// 或 https:// 代理地址。";
  }
}

export function SettingsPage({ app }: Props) {
  const settings = app.payload?.settings;
  const runtime = app.payload?.runtime;

  const [enabled, setEnabled] = useState(false);
  const [url, setUrl] = useState("");
  const [mihomoPath, setMihomoPath] = useState("");

  useEffect(() => {
    if (!settings) return;
    setEnabled(settings.local_proxy_enabled);
    setUrl(settings.local_proxy_url);
    setMihomoPath(settings.mihomo_path);
  }, [settings?.local_proxy_enabled, settings?.local_proxy_url, settings?.mihomo_path]);

  const proxyError = localProxyError(enabled, url);
  const proxyHintId = "local-proxy-hint";

  return (
    <div className={common.page}>
      <header className={common.header}>
        <div className={common.headingGroup}>
          <h1 className={common.title}>设置</h1>
          <p className={common.subtitle}>配置 mihomo 内核路径与客户端自身的本地代理。</p>
        </div>
      </header>

      <div className={common.scroll}>
        <div className={styles.cards}>
          <section className={styles.card}>
            <h2 className={styles.cardHeading}>mihomo 内核</h2>
            <label className="field">
              <span className="fieldLabel">mihomo.exe 文件路径</span>
              <input
                className="input mono"
                placeholder="例如: C:\\Users\\你的用户名\\.mihomo_switch\\mihomo.exe"
                value={mihomoPath}
                onChange={(event) => setMihomoPath(event.target.value)}
              />
              <span className="fieldHint">
                {runtime?.mihomo_exists ? "已检测到内核可执行文件。" : "未检测到内核，请下载 mihomo 并填写完整路径。"}
              </span>
            </label>
          </section>

          <section className={styles.card}>
            <h2 className={styles.cardHeading}>客户端本地代理</h2>
            <label className={styles.toggleRow}>
              <button
                type="button"
                className={enabled ? "switch switchOn" : "switch"}
                role="switch"
                aria-checked={enabled}
                aria-label="启用本地代理"
                onClick={() => setEnabled((prev) => !prev)}
              />
              <span>应用自身访问网络时使用本地代理</span>
            </label>
            <label className="field">
              <span className="fieldLabel">代理地址</span>
              <input
                className={`input mono${proxyError ? " inputError" : ""}`}
                placeholder="例如: http://127.0.0.1:20122"
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                aria-invalid={Boolean(proxyError)}
                aria-describedby={proxyHintId}
              />
              <span id={proxyHintId} role={proxyError ? "alert" : undefined} className={proxyError ? styles.errorHint : "fieldHint"}>
                {proxyError || "留空时使用默认地址 http://127.0.0.1:20122。"}
              </span>
            </label>
          </section>

          {runtime ? (
            <section className={styles.card}>
              <h2 className={styles.cardHeading}>运行时路径</h2>
              <div className={styles.pathRow}>
                <span className={styles.pathLabel}>配置文件</span>
                <span className={`mono ${styles.pathValue}`}>{runtime.config_path || "--"}</span>
              </div>
              <div className={styles.pathRow}>
                <span className={styles.pathLabel}>运行目录</span>
                <span className={`mono ${styles.pathValue}`}>{runtime.runtime_dir || "--"}</span>
              </div>
            </section>
          ) : null}

          <div className={styles.actions}>
            <button
              className="btn btnPrimary"
              disabled={app.busy || Boolean(proxyError)}
              onClick={() => void app.saveProxy(enabled, url.trim(), mihomoPath.trim())}
            >
              保存设置
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
