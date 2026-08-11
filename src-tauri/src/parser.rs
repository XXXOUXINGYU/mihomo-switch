use std::{path::PathBuf, process::Command, time::Duration};

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use reqwest::{Client, Proxy};
use serde_json::Value;
use tauri::Runtime;
use url::Url;

use crate::{
    logging::emit_log,
    models::{default_network, ProxyNode, RealityOptions},
};

const PROXY_UAS: &[&str] = &[
    "clash-verge/2.2.3",
    "clash-verge-rev/v2.0.0",
    "sing-box 1.11.0",
    "Mihomo/1.19.0",
    "clash.meta",
    "ClashforWindows",
    "v2rayN",
];

const CONNECT_TIMEOUT_SECS: u64 = 5;
const REQUEST_TIMEOUT_SECS: u64 = 20;
const CURL_MAX_TIME_SECS: u64 = 60;
const FETCH_TOTAL_TIMEOUT_SECS: u64 = 75;
const MAX_SUBSCRIPTION_BYTES: usize = 16 * 1024 * 1024;

struct CurlFetchResult {
    success: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
}

fn trusted_curl_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot")?;
        let path = PathBuf::from(root).join("System32").join("curl.exe");
        path.is_file().then_some(path)
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from("curl"))
    }
}

fn build_client(proxy_url: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS));
    if let Some(proxy_url) = proxy_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.proxy(Proxy::all(proxy_url).context("invalid local proxy url")?);
    }
    builder.build().map_err(Into::into)
}

fn validate_remote_http_url(value: &str, label: &str) -> Result<()> {
    let parsed = Url::parse(value).with_context(|| format!("{label}地址无效"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(anyhow!("{label}仅支持 http:// 或 https:// 地址"));
    }
    if parsed.host_str().is_none() {
        return Err(anyhow!("{label}缺少有效主机名"));
    }
    Ok(())
}

async fn fetch_text_via_curl(
    url: &str,
    ua: &str,
    proxy_url: Option<&str>,
) -> Result<Option<CurlFetchResult>> {
    let url = url.to_string();
    let ua = ua.to_string();
    let proxy = proxy_url.map(str::to_string);

    tokio::task::spawn_blocking(move || {
        let Some(binary) = trusted_curl_path() else {
            return Ok(None);
        };
        let mut command = Command::new(binary);
        command
            .arg("--silent")
            .arg("--show-error")
            .arg("--location")
            .arg("--proto")
            .arg("=http,https")
            .arg("--proto-redir")
            .arg("=http,https")
            .arg("--max-time")
            .arg(CURL_MAX_TIME_SECS.to_string())
            .arg("--max-filesize")
            .arg(MAX_SUBSCRIPTION_BYTES.to_string())
            .arg("--user-agent")
            .arg(&ua)
            .arg("--header")
            .arg("Accept: */*");
        if let Some(proxy) = proxy.filter(|value| !value.trim().is_empty()) {
            command.arg("--proxy").arg(proxy);
        } else {
            command.arg("--noproxy").arg("*");
        }
        command.arg(&url);

        let output = match command.output() {
            Ok(output) => output,
            Err(_) => return Ok(None),
        };

        Ok(Some(CurlFetchResult {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }))
    })
    .await?
}

async fn response_bytes_limited(mut response: reqwest::Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_SUBSCRIPTION_BYTES as u64)
    {
        return Err(anyhow!(
            "订阅响应超过 {} MB 限制",
            MAX_SUBSCRIPTION_BYTES / 1024 / 1024
        ));
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len().saturating_add(chunk.len()) > MAX_SUBSCRIPTION_BYTES {
            return Err(anyhow!(
                "订阅响应超过 {} MB 限制",
                MAX_SUBSCRIPTION_BYTES / 1024 / 1024
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn response_text_limited(response: reqwest::Response) -> Result<String> {
    Ok(String::from_utf8_lossy(&response_bytes_limited(response).await?).into_owned())
}

fn is_known_image_payload(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(b"\x89PNG\r\n\x1A\n")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
}

fn looks_like_binary_payload(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if is_known_image_payload(bytes) {
        return true;
    }

    let sample = &bytes[..bytes.len().min(512)];
    let printable = sample
        .iter()
        .filter(|byte| matches!(**byte, b'\t' | b'\n' | b'\r' | 0x20..=0x7E))
        .count();

    printable as f32 / (sample.len().max(1) as f32) < 0.65
}

fn content_type_is_image(content_type: Option<&str>) -> bool {
    content_type
        .map(str::trim)
        .map(|value| value.to_ascii_lowercase().starts_with("image/"))
        .unwrap_or(false)
}

fn redact_sensitive_urls(message: &str, urls: &[&str]) -> String {
    let mut redacted = message.to_string();
    for url in urls.iter().copied().filter(|value| !value.is_empty()) {
        redacted = redacted.replace(url, "[订阅链接已隐藏]");
        let encoded = urlencoding::encode(url);
        if encoded.as_ref() != url {
            redacted = redacted.replace(encoded.as_ref(), "[订阅链接已隐藏]");
        }
    }
    redacted
}

fn looks_like_clash_subscription_prefix(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("mixed-port:")
        || trimmed.starts_with("proxies:")
        || (trimmed.contains("\nproxies:") && trimmed.contains("\nproxy-groups:"))
}

fn incomplete_subscription_timeout_error(
    candidate: &str,
    route_label: &str,
    transport: &str,
) -> anyhow::Error {
    anyhow!(
        "UA={candidate} 通过{route_label}/{transport} 已拿到订阅文本开头，但在 {CURL_MAX_TIME_SECS} 秒内未完整传输完毕。\
当前订阅站返回较慢，已停止继续重试以避免耗掉一次性链接。请直接重试导入一次。"
    )
}

fn exhausted_image_placeholder_error(
    candidate: &str,
    route_label: &str,
    transport: &str,
) -> anyhow::Error {
    anyhow!(
        "UA={candidate} 通过{route_label}/{transport}拿到的是图片响应，不是订阅文本。\
当前订阅链接很可能已经过期、次数已用完，或服务端把当前请求识别成图片下载。\
请重新提取一个新链接后立即导入。"
    )
}

fn assess_subscription_text(
    text: &str,
    candidate: &str,
    route_label: &str,
    transport: &str,
    last_error: &mut Option<String>,
) -> Result<Option<String>> {
    match detect_and_parse(text) {
        Ok(nodes) if !nodes.is_empty() => Ok(Some(text.to_string())),
        Ok(_) => {
            *last_error = Some(format!(
                "UA={candidate} 通过{route_label}/{transport}返回空订阅，已尝试下一个 User-Agent"
            ));
            Ok(None)
        }
        Err(error) => {
            *last_error = Some(if looks_like_subscription_text(text) {
                format!("UA={candidate} 通过{route_label}/{transport}返回内容无法解析: {error}")
            } else {
                format!("UA={candidate} 通过{route_label}/{transport}返回了非订阅内容")
            });
            Ok(None)
        }
    }
}

fn candidate_contains(candidates: &[String], value: &str) -> bool {
    candidates
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
}

fn should_prioritize_user_ua(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && !trimmed.eq_ignore_ascii_case("clash.meta")
        && !trimmed.eq_ignore_ascii_case("mihomo-switch")
}

fn build_candidate_uas(ua: Option<&str>) -> Vec<String> {
    let explicit = ua.map(str::trim).filter(|value| !value.is_empty());
    let mut candidates = Vec::new();

    if let Some(value) = explicit {
        if should_prioritize_user_ua(value) && !candidate_contains(&candidates, value) {
            candidates.push(value.to_string());
        }
    }

    for item in PROXY_UAS {
        if !candidate_contains(&candidates, item) {
            candidates.push((*item).to_string());
        }
    }

    if let Some(value) = explicit {
        if !candidate_contains(&candidates, value) {
            candidates.push(value.to_string());
        }
    }

    candidates
}

async fn fetch_parseable_subscription(
    url: &str,
    ua: Option<&str>,
    local_proxy_enabled: bool,
    local_proxy_url: &str,
) -> Result<(String, String)> {
    validate_remote_http_url(url, "订阅")?;
    let candidates = build_candidate_uas(ua);
    let mut last_error = None;
    let proxy_candidates: [Option<&str>; 2] = [None, Some(local_proxy_url)];
    let proxy_iter = if local_proxy_enabled {
        proxy_candidates.as_slice()
    } else {
        &proxy_candidates[..1]
    };

    for proxy in proxy_iter {
        let client = build_client(*proxy)?;
        let route_label = if proxy.is_some() { "proxy" } else { "direct" };
        for candidate in &candidates {
            if let Some(curl_result) = fetch_text_via_curl(url, candidate, *proxy).await? {
                if curl_result.stdout.len() > MAX_SUBSCRIPTION_BYTES {
                    return Err(anyhow!(
                        "订阅响应超过 {} MB 限制",
                        MAX_SUBSCRIPTION_BYTES / 1024 / 1024
                    ));
                }
                if !curl_result.stdout.is_empty() {
                    if looks_like_binary_payload(&curl_result.stdout) {
                        return Err(exhausted_image_placeholder_error(
                            candidate,
                            route_label,
                            "curl",
                        ));
                    }

                    let text = String::from_utf8_lossy(&curl_result.stdout).into_owned();
                    if let Some(text) = assess_subscription_text(
                        &text,
                        candidate,
                        route_label,
                        "curl",
                        &mut last_error,
                    )? {
                        return Ok((text, format!("{candidate} via {route_label}/curl")));
                    }

                    if !curl_result.success
                        && curl_result.exit_code == Some(28)
                        && looks_like_clash_subscription_prefix(&text)
                    {
                        return Err(incomplete_subscription_timeout_error(
                            candidate,
                            route_label,
                            "curl",
                        ));
                    }
                }

                if !curl_result.success {
                    let stderr = if curl_result.stderr.is_empty() {
                        match curl_result.exit_code {
                            Some(code) => format!("curl exit code {code}"),
                            None => "curl terminated without an exit code".to_string(),
                        }
                    } else {
                        curl_result.stderr
                    };
                    last_error = Some(redact_sensitive_urls(
                        &format!("UA={candidate} 通过{route_label}/curl请求失败: {stderr}"),
                        &[url],
                    ));
                }
            }

            match client.get(url).header("User-Agent", candidate).send().await {
                Ok(response) => {
                    let content_type = response
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                    let body = response_bytes_limited(response).await?;
                    if content_type_is_image(content_type.as_deref())
                        || looks_like_binary_payload(&body)
                    {
                        return Err(exhausted_image_placeholder_error(
                            candidate,
                            route_label,
                            "reqwest",
                        ));
                    }

                    let text = String::from_utf8_lossy(&body).into_owned();

                    if let Some(text) = assess_subscription_text(
                        &text,
                        candidate,
                        route_label,
                        "reqwest",
                        &mut last_error,
                    )? {
                        return Ok((text, format!("{candidate} via {route_label}")));
                    }
                }
                Err(error) => {
                    last_error = Some(redact_sensitive_urls(
                        &format!("UA={candidate} 通过{route_label}请求失败: {error}"),
                        &[url],
                    ));
                }
            }
        }
    }

    let message = format!(
        "所有 User-Agent 均获取失败，请尝试配置订阅转换服务或改用手动导入。{}",
        last_error.unwrap_or_default()
    );
    Err(anyhow!(message))
}

pub async fn fetch_subscription<R: Runtime>(
    app: &tauri::AppHandle<R>,
    url: &str,
    ua: Option<&str>,
    subconverter: &str,
    local_proxy_enabled: bool,
    local_proxy_url: &str,
) -> Result<String> {
    match tokio::time::timeout(Duration::from_secs(FETCH_TOTAL_TIMEOUT_SECS), async {
        if !subconverter.trim().is_empty() {
            return fetch_via_subconverter(
                app,
                url,
                subconverter,
                local_proxy_enabled,
                local_proxy_url,
            )
            .await
            .map(|text| (text, "subconverter".to_string()));
        }
        fetch_parseable_subscription(url, ua, local_proxy_enabled, local_proxy_url).await
    })
    .await
    {
        Ok(result) => {
            let (text, candidate) = result?;
            emit_log(app, "info", format!("订阅获取成功: UA={candidate}"));
            Ok(text)
        }
        Err(_) => {
            let message = format!("订阅拉取超时（>{FETCH_TOTAL_TIMEOUT_SECS} 秒）");
            emit_log(app, "error", &message);
            Err(anyhow!(message))
        }
    }
}

pub async fn fetch_subscription_headless(
    url: &str,
    ua: Option<&str>,
    subconverter: &str,
    local_proxy_enabled: bool,
    local_proxy_url: &str,
) -> Result<String> {
    match tokio::time::timeout(Duration::from_secs(FETCH_TOTAL_TIMEOUT_SECS), async {
        if !subconverter.trim().is_empty() {
            return fetch_via_subconverter_headless(
                url,
                subconverter,
                local_proxy_enabled,
                local_proxy_url,
            )
            .await
            .map(|text| (text, "subconverter".to_string()));
        }
        fetch_parseable_subscription(url, ua, local_proxy_enabled, local_proxy_url).await
    })
    .await
    {
        Ok(result) => result.map(|(text, _)| text),
        Err(_) => Err(anyhow!("订阅拉取超时（>{FETCH_TOTAL_TIMEOUT_SECS} 秒）")),
    }
}

async fn fetch_via_subconverter<R: Runtime>(
    app: &tauri::AppHandle<R>,
    url: &str,
    subconverter: &str,
    local_proxy_enabled: bool,
    local_proxy_url: &str,
) -> Result<String> {
    validate_remote_http_url(url, "订阅")?;
    validate_remote_http_url(subconverter, "订阅转换服务")?;
    let encoded = urlencoding::encode(url);
    let full_url = format!("{subconverter}{encoded}");
    let proxy = local_proxy_enabled.then_some(local_proxy_url);

    let client = build_client(proxy)?;
    let response = client
        .get(&full_url)
        .header("User-Agent", "clash.meta")
        .send()
        .await
        .map_err(|error| {
            anyhow!(
                "通过订阅转换服务获取失败: {}",
                redact_sensitive_urls(&error.to_string(), &[url, &full_url])
            )
        })?;
    emit_log(app, "info", "订阅转换服务请求成功");
    response_text_limited(response).await
}

async fn fetch_via_subconverter_headless(
    url: &str,
    subconverter: &str,
    local_proxy_enabled: bool,
    local_proxy_url: &str,
) -> Result<String> {
    validate_remote_http_url(url, "订阅")?;
    validate_remote_http_url(subconverter, "订阅转换服务")?;
    let encoded = urlencoding::encode(url);
    let full_url = format!("{subconverter}{encoded}");
    let proxy = local_proxy_enabled.then_some(local_proxy_url);

    let client = build_client(proxy)?;
    let response = client
        .get(&full_url)
        .header("User-Agent", "clash.meta")
        .send()
        .await
        .map_err(|error| {
            anyhow!(
                "通过订阅转换服务获取失败: {}",
                redact_sensitive_urls(&error.to_string(), &[url, &full_url])
            )
        })?;
    response_text_limited(response).await
}

fn looks_like_subscription_text(text: &str) -> bool {
    if text.len() < 10 {
        return false;
    }
    let sample: String = text.chars().take(500).collect();
    let printable = sample
        .chars()
        .filter(|ch| ch.is_ascii_graphic() || ch.is_whitespace() || !ch.is_ascii())
        .count();
    printable as f32 / sample.chars().count().max(1) as f32 > 0.85
}

pub fn detect_and_parse(raw_text: &str) -> Result<Vec<ProxyNode>> {
    let text = raw_text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }

    if let Ok(nodes) = try_parse_singbox_json(text) {
        if !nodes.is_empty() {
            return Ok(nodes);
        }
    }

    if let Ok(nodes) = try_parse_clash_yaml(text) {
        if !nodes.is_empty() {
            return Ok(nodes);
        }
    }

    if let Ok(nodes) = try_parse_base64_uris(text) {
        if !nodes.is_empty() {
            return Ok(nodes);
        }
    }

    let nodes = try_parse_plain_uris(text)?;
    if !nodes.is_empty() {
        return Ok(nodes);
    }

    Err(anyhow!(
        "无法识别订阅格式，支持 sing-box JSON / Clash YAML / base64 URI / 逐行 URI"
    ))
}

fn try_parse_singbox_json(text: &str) -> Result<Vec<ProxyNode>> {
    let data: Value = serde_json::from_str(text)?;
    let outbounds = data
        .get("outbounds")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("no outbounds"))?;

    let mut nodes = Vec::new();
    for outbound in outbounds {
        if let Some(node) = normalize_singbox_outbound(outbound)? {
            nodes.push(node);
        }
    }
    Ok(nodes)
}

fn normalize_singbox_outbound(outbound: &Value) -> Result<Option<ProxyNode>> {
    let node_type = outbound
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase();
    let clash_type = match node_type.as_str() {
        "vmess" | "vless" | "trojan" => node_type.as_str(),
        "shadowsocks" => "ss",
        _ => return Ok(None),
    };

    let Some(server) = outbound.get("server").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(port) = outbound.get("server_port").and_then(Value::as_u64) else {
        return Ok(None);
    };

    let name = outbound
        .get("tag")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("{server}:{port}"));

    let mut raw = serde_json::Map::<String, Value>::new();
    raw.insert("name".into(), Value::String(name));
    raw.insert("type".into(), Value::String(clash_type.to_string()));
    raw.insert("server".into(), Value::String(server.to_string()));
    raw.insert("port".into(), Value::Number(port.into()));

    match clash_type {
        "vmess" => {
            let Some(uuid) = outbound.get("uuid").and_then(Value::as_str) else {
                return Ok(None);
            };
            raw.insert("uuid".into(), Value::String(uuid.to_string()));
            raw.insert(
                "alterId".into(),
                Value::Number(
                    outbound
                        .get("alter_id")
                        .or_else(|| outbound.get("alterId"))
                        .and_then(Value::as_i64)
                        .unwrap_or_default()
                        .into(),
                ),
            );
            raw.insert(
                "cipher".into(),
                Value::String(
                    outbound
                        .get("security")
                        .and_then(Value::as_str)
                        .unwrap_or("auto")
                        .to_string(),
                ),
            );
            raw.insert("udp".into(), Value::Bool(true));
        }
        "vless" => {
            let Some(uuid) = outbound.get("uuid").and_then(Value::as_str) else {
                return Ok(None);
            };
            raw.insert("uuid".into(), Value::String(uuid.to_string()));
            raw.insert("udp".into(), Value::Bool(true));
            raw.insert("encryption".into(), Value::String("none".to_string()));
            if let Some(flow) = outbound.get("flow").and_then(Value::as_str) {
                raw.insert("flow".into(), Value::String(flow.to_string()));
            }
        }
        "trojan" => {
            let Some(password) = outbound.get("password").and_then(Value::as_str) else {
                return Ok(None);
            };
            raw.insert("password".into(), Value::String(password.to_string()));
            raw.insert("udp".into(), Value::Bool(true));
        }
        "ss" => {
            let Some(method) = outbound.get("method").and_then(Value::as_str) else {
                return Ok(None);
            };
            let Some(password) = outbound.get("password").and_then(Value::as_str) else {
                return Ok(None);
            };
            raw.insert("cipher".into(), Value::String(method.to_string()));
            raw.insert("password".into(), Value::String(password.to_string()));
            raw.insert("udp".into(), Value::Bool(true));
            if let Some(plugin) = outbound.get("plugin").and_then(Value::as_str) {
                raw.insert("plugin".into(), Value::String(plugin.to_string()));
            }
            if let Some(plugin_opts) = outbound.get("plugin_opts").and_then(Value::as_str) {
                raw.insert("plugin-opts".into(), Value::String(plugin_opts.to_string()));
            }
        }
        _ => return Ok(None),
    }

    apply_singbox_tls(&mut raw, clash_type, outbound.get("tls"));
    apply_singbox_transport(&mut raw, outbound.get("transport"));

    let raw_value = prune_json_value(Value::Object(raw));
    Ok(Some(normalize_clash_proxy(&raw_value)?))
}

fn apply_singbox_tls(
    raw: &mut serde_json::Map<String, Value>,
    node_type: &str,
    tls: Option<&Value>,
) {
    let tls_object = tls.and_then(Value::as_object);
    let reality_enabled = tls_object
        .and_then(|tls| tls.get("reality"))
        .and_then(Value::as_object)
        .and_then(|reality| reality.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let tls_enabled = tls_object
        .and_then(|tls| tls.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(matches!(node_type, "trojan") || reality_enabled);

    if tls_enabled {
        raw.insert("tls".into(), Value::Bool(true));
    }

    if let Some(insecure) = tls_object
        .and_then(|tls| tls.get("insecure"))
        .and_then(Value::as_bool)
    {
        raw.insert("skip-cert-verify".into(), Value::Bool(insecure));
    }

    if let Some(server_name) = tls_object
        .and_then(|tls| tls.get("server_name"))
        .and_then(Value::as_str)
    {
        raw.insert("servername".into(), Value::String(server_name.to_string()));
    }

    if let Some(fingerprint) = tls_object
        .and_then(|tls| tls.get("utls"))
        .and_then(Value::as_object)
        .and_then(|utls| utls.get("fingerprint"))
        .and_then(Value::as_str)
    {
        raw.insert(
            "client-fingerprint".into(),
            Value::String(fingerprint.to_string()),
        );
    }

    if let Some(reality) = tls_object
        .and_then(|tls| tls.get("reality"))
        .and_then(Value::as_object)
    {
        let public_key = reality
            .get("public_key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let short_id = reality
            .get("short_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !public_key.is_empty() || !short_id.is_empty() {
            raw.insert(
                "reality-opts".into(),
                serde_json::json!({
                    "public-key": public_key,
                    "short-id": short_id,
                }),
            );
        }
    }
}

fn apply_singbox_transport(raw: &mut serde_json::Map<String, Value>, transport: Option<&Value>) {
    let Some(transport) = transport.and_then(Value::as_object) else {
        return;
    };
    let Some(transport_type) = transport.get("type").and_then(Value::as_str) else {
        return;
    };

    match transport_type {
        "ws" => {
            raw.insert("network".into(), Value::String("ws".to_string()));
            let mut ws_opts = serde_json::Map::<String, Value>::new();
            if let Some(path) = transport.get("path").and_then(Value::as_str) {
                ws_opts.insert("path".into(), Value::String(path.to_string()));
            }
            if let Some(headers) = transport.get("headers").and_then(Value::as_object) {
                ws_opts.insert("headers".into(), Value::Object(headers.clone()));
            }
            if let Some(max_early_data) = transport.get("max_early_data").and_then(Value::as_u64) {
                ws_opts.insert(
                    "max-early-data".into(),
                    Value::Number(max_early_data.into()),
                );
            }
            if let Some(header_name) = transport
                .get("early_data_header_name")
                .and_then(Value::as_str)
            {
                ws_opts.insert(
                    "early-data-header-name".into(),
                    Value::String(header_name.to_string()),
                );
            }
            raw.insert("ws-opts".into(), Value::Object(ws_opts));
        }
        "grpc" => {
            raw.insert("network".into(), Value::String("grpc".to_string()));
            let mut grpc_opts = serde_json::Map::<String, Value>::new();
            if let Some(service_name) = transport.get("service_name").and_then(Value::as_str) {
                grpc_opts.insert(
                    "grpc-service-name".into(),
                    Value::String(service_name.to_string()),
                );
            }
            raw.insert("grpc-opts".into(), Value::Object(grpc_opts));
        }
        "httpupgrade" => {
            raw.insert("network".into(), Value::String("httpupgrade".to_string()));
            let mut opts = serde_json::Map::<String, Value>::new();
            if let Some(host) = transport.get("host").and_then(Value::as_str) {
                opts.insert("host".into(), Value::String(host.to_string()));
            }
            if let Some(path) = transport.get("path").and_then(Value::as_str) {
                opts.insert("path".into(), Value::String(path.to_string()));
            }
            if let Some(headers) = transport.get("headers").and_then(Value::as_object) {
                opts.insert("headers".into(), Value::Object(headers.clone()));
            }
            raw.insert("httpupgrade-opts".into(), Value::Object(opts));
        }
        "http" => {
            raw.insert("network".into(), Value::String("http".to_string()));
            let mut opts = serde_json::Map::<String, Value>::new();
            if let Some(hosts) = transport.get("host").and_then(Value::as_array) {
                opts.insert(
                    "headers".into(),
                    serde_json::json!({
                        "Host": hosts,
                    }),
                );
            }
            if let Some(path) = transport.get("path").and_then(Value::as_str) {
                opts.insert(
                    "path".into(),
                    Value::Array(vec![Value::String(path.to_string())]),
                );
            }
            raw.insert("http-opts".into(), Value::Object(opts));
        }
        "quic" => {
            raw.insert("network".into(), Value::String("quic".to_string()));
        }
        _ => {}
    }
}

fn prune_json_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut next = serde_json::Map::new();
            for (key, value) in map {
                let pruned = prune_json_value(value);
                match &pruned {
                    Value::Null => continue,
                    Value::String(text) if text.is_empty() => continue,
                    Value::Array(values) if values.is_empty() => continue,
                    Value::Object(values) if values.is_empty() => continue,
                    _ => {
                        next.insert(key, pruned);
                    }
                }
            }
            Value::Object(next)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(prune_json_value).collect()),
        other => other,
    }
}

fn try_parse_clash_yaml(text: &str) -> Result<Vec<ProxyNode>> {
    let data: Value = serde_yaml::from_str(text)?;
    let proxies = data
        .get("proxies")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("no proxies"))?;

    let mut nodes = Vec::new();
    for proxy in proxies {
        if let Some(server) = proxy.get("server").and_then(Value::as_str) {
            if server == "127.0.0.1" || server == "localhost" || server.is_empty() {
                continue;
            }
        }
        nodes.push(normalize_clash_proxy(proxy)?);
    }
    Ok(nodes)
}

fn normalize_clash_proxy(proxy: &Value) -> Result<ProxyNode> {
    let node_type = proxy
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase();
    let reality_opts = proxy.get("reality-opts").map(|value| RealityOptions {
        public_key: value
            .get("public-key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        short_id: value
            .get("short-id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    });

    Ok(ProxyNode {
        name: proxy
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        node_type: node_type.clone(),
        server: proxy
            .get("server")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        port: proxy
            .get("port")
            .and_then(Value::as_u64)
            .unwrap_or_default() as u16,
        uuid: proxy
            .get("uuid")
            .or_else(|| proxy.get("password"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        alter_id: proxy
            .get("alterId")
            .and_then(Value::as_i64)
            .unwrap_or_default() as i32,
        cipher: proxy
            .get("cipher")
            .and_then(Value::as_str)
            .unwrap_or("auto")
            .to_string(),
        udp: proxy.get("udp").and_then(Value::as_bool).unwrap_or(true),
        flow: proxy
            .get("flow")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        encryption: proxy
            .get("encryption")
            .and_then(Value::as_str)
            .unwrap_or("none")
            .to_string(),
        tls: proxy
            .get("tls")
            .and_then(Value::as_bool)
            .unwrap_or(matches!(node_type.as_str(), "trojan" | "vless")),
        skip_cert_verify: proxy
            .get("skip-cert-verify")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        servername: proxy
            .get("servername")
            .or_else(|| proxy.get("sni"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        reality_opts,
        client_fingerprint: proxy
            .get("client-fingerprint")
            .and_then(Value::as_str)
            .unwrap_or("chrome")
            .to_string(),
        network: proxy
            .get("network")
            .and_then(Value::as_str)
            .unwrap_or("tcp")
            .to_string(),
        raw: Some(proxy.clone()),
    })
}

fn try_parse_base64_uris(text: &str) -> Result<Vec<ProxyNode>> {
    let mut padded = text.replace('-', "+").replace('_', "/");
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let decoded = STANDARD
        .decode(padded)
        .context("decode base64 subscription")?;
    let content = String::from_utf8_lossy(&decoded).to_string();
    try_parse_plain_uris(&content)
}

fn try_parse_plain_uris(text: &str) -> Result<Vec<ProxyNode>> {
    let mut nodes = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if let Some(node) = parse_single_uri(line)? {
            nodes.push(node);
        }
    }
    Ok(nodes)
}

fn parse_single_uri(uri: &str) -> Result<Option<ProxyNode>> {
    if uri.starts_with("vless://") {
        return Ok(Some(parse_vless_uri(uri)?));
    }
    if uri.starts_with("vmess://") {
        return Ok(Some(parse_vmess_uri(uri)?));
    }
    if uri.starts_with("trojan://") {
        return Ok(Some(parse_trojan_uri(uri)?));
    }
    if uri.starts_with("ss://") {
        return Ok(Some(parse_ss_uri(uri)?));
    }
    Ok(None)
}

fn parse_vless_uri(uri: &str) -> Result<ProxyNode> {
    let body = &uri["vless://".len()..];
    let (body, name) = split_fragment(body);
    let re = Regex::new(r"^([^@]+)@([^:]+):(\d+)$")?;
    let endpoint = body.split('?').next().unwrap_or_default();
    let captures = re
        .captures(endpoint)
        .ok_or_else(|| anyhow!("invalid vless uri"))?;
    let uuid = captures
        .get(1)
        .map(|m| m.as_str())
        .unwrap_or_default()
        .to_string();
    let server = captures
        .get(2)
        .map(|m| m.as_str())
        .unwrap_or_default()
        .to_string();
    let port = captures
        .get(3)
        .map(|m| m.as_str())
        .unwrap_or("0")
        .parse::<u16>()?;
    let params = parse_query(body);
    let security = params
        .get("security")
        .cloned()
        .unwrap_or_else(|| "none".to_string());
    let is_reality = security == "reality";

    Ok(ProxyNode {
        name: if name.is_empty() {
            format!("{server}:{port}")
        } else {
            name
        },
        node_type: "vless".into(),
        server,
        port,
        uuid,
        alter_id: 0,
        cipher: "auto".into(),
        udp: true,
        flow: params.get("flow").cloned().unwrap_or_default(),
        encryption: params
            .get("encryption")
            .cloned()
            .unwrap_or_else(|| "none".into()),
        tls: security == "tls" || security == "reality",
        skip_cert_verify: true,
        servername: params.get("sni").cloned().unwrap_or_default(),
        reality_opts: if is_reality {
            Some(RealityOptions {
                public_key: params.get("pbk").cloned().unwrap_or_default(),
                short_id: params.get("sid").cloned().unwrap_or_default(),
            })
        } else {
            None
        },
        client_fingerprint: params.get("fp").cloned().unwrap_or_else(|| "chrome".into()),
        network: params.get("type").cloned().unwrap_or_else(default_network),
        raw: None,
    })
}

fn parse_vmess_uri(uri: &str) -> Result<ProxyNode> {
    let payload = &uri["vmess://".len()..];
    let mut padded = payload.to_string();
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let decoded = STANDARD.decode(padded)?;
    let value: Value = serde_json::from_slice(&decoded)?;
    Ok(ProxyNode {
        name: value
            .get("ps")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        node_type: "vmess".into(),
        server: value
            .get("add")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        port: value
            .get("port")
            .and_then(Value::as_str)
            .unwrap_or("0")
            .parse::<u16>()
            .unwrap_or_default(),
        uuid: value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        alter_id: value
            .get("aid")
            .and_then(Value::as_str)
            .unwrap_or("0")
            .parse()
            .unwrap_or_default(),
        cipher: value
            .get("scy")
            .and_then(Value::as_str)
            .unwrap_or("auto")
            .to_string(),
        udp: true,
        flow: String::new(),
        encryption: String::new(),
        tls: value
            .get("tls")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .eq_ignore_ascii_case("tls"),
        skip_cert_verify: true,
        servername: value
            .get("sni")
            .or_else(|| value.get("host"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        reality_opts: None,
        client_fingerprint: "chrome".into(),
        network: value
            .get("net")
            .and_then(Value::as_str)
            .unwrap_or("tcp")
            .to_string(),
        raw: Some(value),
    })
}

fn parse_trojan_uri(uri: &str) -> Result<ProxyNode> {
    let body = &uri["trojan://".len()..];
    let (body, name) = split_fragment(body);
    let parsed = Url::parse(&format!(
        "trojan://{}",
        body.split('?').next().unwrap_or_default()
    ))?;
    let password = parsed.username().to_string();
    let server = parsed.host_str().unwrap_or_default().to_string();
    let port = parsed.port().unwrap_or(443);
    let params = parse_query(body);

    Ok(ProxyNode {
        name: if name.is_empty() {
            format!("{server}:{port}")
        } else {
            name
        },
        node_type: "trojan".into(),
        server,
        port,
        uuid: password,
        alter_id: 0,
        cipher: String::new(),
        udp: true,
        flow: String::new(),
        encryption: String::new(),
        tls: true,
        skip_cert_verify: params
            .get("allowInsecure")
            .map(|v| v == "1")
            .unwrap_or(false),
        servername: params.get("sni").cloned().unwrap_or_default(),
        reality_opts: None,
        client_fingerprint: "chrome".into(),
        network: params.get("type").cloned().unwrap_or_else(default_network),
        raw: None,
    })
}

fn parse_ss_uri(uri: &str) -> Result<ProxyNode> {
    let body = &uri["ss://".len()..];
    let (body, name) = split_fragment(body);
    let (cipher, password, host_info) =
        if body.contains('@') && body.split('@').next().unwrap_or_default().contains(':') {
            let (userinfo, hostinfo) = body.split_once('@').unwrap();
            let decoded = decode_if_base64(userinfo);
            let (cipher, password) = decoded
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid ss userinfo"))?;
            (
                cipher.to_string(),
                password.to_string(),
                hostinfo.to_string(),
            )
        } else {
            let payload = body.split('?').next().unwrap_or_default();
            let decoded = String::from_utf8(STANDARD.decode(pad_base64(payload))?)?;
            let (userinfo, hostinfo) = decoded
                .split_once('@')
                .ok_or_else(|| anyhow!("invalid ss payload"))?;
            let (cipher, password) = userinfo
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid ss payload"))?;
            (
                cipher.to_string(),
                password.to_string(),
                hostinfo.to_string(),
            )
        };

    let parsed = Url::parse(&format!("ss://{host_info}"))?;
    let server = parsed.host_str().unwrap_or_default().to_string();
    let port = parsed.port().unwrap_or_default();

    Ok(ProxyNode {
        name: if name.is_empty() {
            format!("{server}:{port}")
        } else {
            name
        },
        node_type: "ss".into(),
        server,
        port,
        uuid: password,
        alter_id: 0,
        cipher,
        udp: true,
        flow: String::new(),
        encryption: String::new(),
        tls: false,
        skip_cert_verify: false,
        servername: String::new(),
        reality_opts: None,
        client_fingerprint: String::new(),
        network: "tcp".into(),
        raw: None,
    })
}

fn split_fragment(body: &str) -> (String, String) {
    if let Some((head, tail)) = body.rsplit_once('#') {
        (
            head.to_string(),
            urlencoding::decode(tail).unwrap_or_default().to_string(),
        )
    } else {
        (body.to_string(), String::new())
    }
}

fn parse_query(body: String) -> std::collections::BTreeMap<String, String> {
    let query = body
        .split_once('?')
        .map(|(_, tail)| tail)
        .unwrap_or_default();
    url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn pad_base64(value: &str) -> String {
    let mut padded = value.replace('-', "+").replace('_', "/");
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    padded
}

fn decode_if_base64(value: &str) -> String {
    let padded = pad_base64(value);
    STANDARD
        .decode(padded)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread,
    };

    #[test]
    fn parses_clash_yaml_and_skips_localhost_nodes() {
        let text = r#"
proxies:
  - name: keep-me
    type: trojan
    server: example.com
    port: 443
    password: secret
  - name: skip-me
    type: trojan
    server: 127.0.0.1
    port: 7890
    password: secret
"#;

        let nodes = detect_and_parse(text).expect("parse clash yaml");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "keep-me");
        assert_eq!(nodes[0].server, "example.com");
    }

    #[test]
    fn parses_singbox_json_outbounds() {
        let text = r#"
{
  "outbounds": [
    {
      "type": "direct",
      "tag": "direct"
    },
    {
      "type": "vless",
      "tag": "VLESS Reality",
      "server": "example.com",
      "server_port": 443,
      "uuid": "11111111-1111-1111-1111-111111111111",
      "flow": "xtls-rprx-vision",
      "tls": {
        "enabled": true,
        "server_name": "cdn.example.com",
        "insecure": true,
        "utls": {
          "enabled": true,
          "fingerprint": "chrome"
        },
        "reality": {
          "enabled": true,
          "public_key": "test-public-key",
          "short_id": "abcd"
        }
      },
      "transport": {
        "type": "ws",
        "path": "/ws",
        "headers": {
          "Host": "cdn.example.com"
        }
      }
    }
  ]
}
"#;

        let nodes = detect_and_parse(text).expect("parse sing-box json");

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "VLESS Reality");
        assert_eq!(nodes[0].node_type, "vless");
        assert_eq!(nodes[0].server, "example.com");
        assert!(nodes[0].tls);
        assert_eq!(nodes[0].servername, "cdn.example.com");
        assert_eq!(nodes[0].network, "ws");
        assert_eq!(
            nodes[0]
                .reality_opts
                .as_ref()
                .map(|opts| opts.public_key.as_str()),
            Some("test-public-key")
        );
        assert_eq!(
            nodes[0]
                .raw
                .as_ref()
                .and_then(|raw| raw.get("ws-opts"))
                .and_then(|value| value.get("path"))
                .and_then(Value::as_str),
            Some("/ws")
        );
    }

    #[test]
    fn parses_plain_vless_uri() {
        let text = "vless://11111111-1111-1111-1111-111111111111@example.com:443?security=reality&pbk=test-key&sid=abcd&fp=chrome&type=tcp#Test%20Node";
        let nodes = detect_and_parse(text).expect("parse vless");

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "Test Node");
        assert_eq!(nodes[0].node_type, "vless");
        assert!(nodes[0].tls);
        assert_eq!(
            nodes[0]
                .reality_opts
                .as_ref()
                .map(|opts| opts.public_key.as_str()),
            Some("test-key")
        );
    }

    #[test]
    fn parses_base64_vmess_subscription() {
        let vmess = r#"{"v":"2","ps":"VMESS Test","add":"vmess.example.com","port":"443","id":"11111111-1111-1111-1111-111111111111","aid":"0","net":"ws","tls":"tls"}"#;
        let encoded_uri = format!("vmess://{}", STANDARD.encode(vmess));
        let subscription = STANDARD.encode(encoded_uri);
        let nodes = detect_and_parse(&subscription).expect("parse base64 vmess");

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "VMESS Test");
        assert_eq!(nodes[0].node_type, "vmess");
        assert_eq!(nodes[0].server, "vmess.example.com");
        assert!(nodes[0].tls);
    }

    fn serve_subscription_by_ua(routes: &[(&str, &str, &str)]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind multi-ua test server");
        let address = listener.local_addr().expect("multi-ua server address");
        let response_routes: Vec<(String, String, String)> = routes
            .iter()
            .map(|(ua, body, content_type)| {
                (
                    (*ua).to_string(),
                    (*body).to_string(),
                    (*content_type).to_string(),
                )
            })
            .collect();

        thread::spawn(move || {
            for _ in 0..8 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).unwrap_or_default();
                let request = String::from_utf8_lossy(&buffer[..read]);
                let user_agent = request.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("User-Agent")
                            .then_some(value.trim().to_string())
                    })
                });

                let (body, content_type) = user_agent
                    .as_deref()
                    .and_then(|ua| {
                        response_routes
                            .iter()
                            .find(|(candidate, _, _)| candidate.eq_ignore_ascii_case(ua))
                            .map(|(_, body, content_type)| (body.as_str(), content_type.as_str()))
                    })
                    .unwrap_or(("禁止访问", "text/plain; charset=utf-8"));

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        format!("http://{address}/subscription")
    }

    fn serve_subscription_bytes_by_ua(routes: &[(&str, &[u8], &str)]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind binary multi-ua test server");
        let address = listener
            .local_addr()
            .expect("binary multi-ua server address");
        let response_routes: Vec<(String, Vec<u8>, String)> = routes
            .iter()
            .map(|(ua, body, content_type)| {
                (
                    (*ua).to_string(),
                    (*body).to_vec(),
                    (*content_type).to_string(),
                )
            })
            .collect();

        thread::spawn(move || {
            for _ in 0..8 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).unwrap_or_default();
                let request = String::from_utf8_lossy(&buffer[..read]);
                let user_agent = request.lines().find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("User-Agent")
                            .then_some(value.trim().to_string())
                    })
                });

                let (body, content_type) = user_agent
                    .as_deref()
                    .and_then(|ua| {
                        response_routes
                            .iter()
                            .find(|(candidate, _, _)| candidate.eq_ignore_ascii_case(ua))
                            .map(|(_, body, content_type)| (body.as_slice(), content_type.as_str()))
                    })
                    .unwrap_or((&b"forbidden"[..], "application/octet-stream"));

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
            }
        });

        format!("http://{address}/subscription")
    }

    fn serve_counted_jpeg_placeholder(counter: Arc<AtomicUsize>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind counted jpeg test server");
        let address = listener.local_addr().expect("counted jpeg server address");
        let jpeg_bytes = b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01";

        thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer);

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    jpeg_bytes.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(jpeg_bytes);
                let _ = stream.flush();
            }
        });

        format!("http://{address}/subscription")
    }

    #[test]
    fn candidate_uas_prioritize_clash_verge_before_legacy_default() {
        let candidates = build_candidate_uas(Some("clash.meta"));

        assert_eq!(
            candidates.first().map(String::as_str),
            Some("clash-verge/2.2.3")
        );
        assert!(candidates.iter().any(|ua| ua == "clash.meta"));
    }

    #[test]
    fn subscription_errors_redact_raw_and_encoded_urls() {
        let url = "https://example.com/sub/secret-token?token=private-value";
        let encoded = urlencoding::encode(url);
        let message = format!("request {url} failed; converter target={encoded}");
        let redacted = redact_sensitive_urls(&message, &[url]);

        assert!(!redacted.contains("secret-token"));
        assert!(!redacted.contains("private-value"));
        assert_eq!(redacted.matches("[订阅链接已隐藏]").count(), 2);
    }

    #[test]
    fn subscription_fetch_rejects_non_http_protocols() {
        let error = validate_remote_http_url("file:///etc/passwd", "订阅")
            .expect_err("file URLs must be rejected");
        assert!(error.to_string().contains("仅支持 http:// 或 https://"));
    }

    #[tokio::test]
    async fn fetch_subscription_headless_skips_text_like_but_unparseable_response() {
        let url = serve_subscription_by_ua(&[
            (
                "clash-verge/2.2.3",
                "<html><body>access denied</body></html>",
                "text/html; charset=utf-8",
            ),
            (
                "clash-verge-rev/v2.0.0",
                "proxies:\n  - name: UA Fallback Trojan\n    type: trojan\n    server: example.com\n    port: 443\n    password: secret\n",
                "text/plain; charset=utf-8",
            ),
        ]);

        let text = fetch_subscription_headless(&url, Some("clash.meta"), "", false, "")
            .await
            .expect("fetch subscription with ua fallback");
        let nodes = detect_and_parse(&text).expect("parse fallback response");

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "UA Fallback Trojan");
        assert_eq!(nodes[0].node_type, "trojan");
    }

    #[tokio::test]
    async fn fetch_subscription_headless_fails_fast_on_jpeg_placeholder() {
        let request_count = Arc::new(AtomicUsize::new(0));
        let url = serve_counted_jpeg_placeholder(request_count.clone());

        let error = fetch_subscription_headless(&url, Some("clash.meta"), "", false, "")
            .await
            .expect_err("jpeg placeholder should fail fast");

        assert!(error.to_string().contains("图片响应"));
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
    }
}
