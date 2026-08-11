use chrono::Local;
use regex::{Captures, Regex};
use tauri::{AppHandle, Emitter, Runtime};

use crate::models::{LatencyResult, LogEntry};

const MAX_LOG_CHARS: usize = 4_000;

fn redact_url(raw: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(raw) else {
        return raw.to_string();
    };
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("[redacted]");
    }
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("[redacted]"));
    }
    parsed.set_path("/[redacted]");
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

fn sanitize_log_message(message: &str) -> String {
    let url_pattern = Regex::new(
        r#"(?i)\b(?:https?|trojan|vless|vmess|ss|ssr|hysteria2?|hy2|tuic)://[^\s"'<>]+"#,
    )
    .expect("valid URL redaction regex");
    let secret_pattern =
        Regex::new(r#"(?i)\b(password|passwd|token|secret|uuid)\b(\s*[:=]\s*)([^\s,;]+)"#)
            .expect("valid secret redaction regex");
    let urls_redacted =
        url_pattern.replace_all(message, |captures: &Captures<'_>| redact_url(&captures[0]));
    let secrets_redacted = secret_pattern.replace_all(&urls_redacted, "$1$2[redacted]");
    let mut result: String = secrets_redacted.chars().take(MAX_LOG_CHARS).collect();
    if secrets_redacted.chars().count() > MAX_LOG_CHARS {
        result.push_str("…[日志已截断]");
    }
    result
}

pub fn emit_log<R: Runtime>(app: &AppHandle<R>, level: &str, message: impl Into<String>) {
    let entry = LogEntry {
        level: level.to_string(),
        message: sanitize_log_message(&message.into()),
        timestamp: Local::now().format("%H:%M:%S").to_string(),
    };
    let _ = app.emit("runtime-log", entry);
}

pub fn emit_latency_result<R: Runtime>(app: &AppHandle<R>, result: &LatencyResult) {
    let _ = app.emit("latency-result", result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_logs_hide_credentials_and_url_queries() {
        let message =
            "request https://user:pass@example.com/sub?token=private failed password: secret";
        let sanitized = sanitize_log_message(message);

        assert!(!sanitized.contains("private"));
        assert!(!sanitized.contains("/sub"));
        assert!(!sanitized.contains("pass@"));
        assert!(!sanitized.contains("password: secret"));
        assert!(sanitized.contains("[redacted]"));
    }

    #[test]
    fn runtime_logs_hide_proxy_uri_credentials() {
        let sanitized =
            sanitize_log_message("failed proxy trojan://password@example.com:443?security=tls");
        assert!(!sanitized.contains("password"));
        assert!(!sanitized.contains("security=tls"));
        assert!(sanitized.contains("[redacted]"));
    }

    #[test]
    fn runtime_logs_are_bounded() {
        let sanitized = sanitize_log_message(&"x".repeat(MAX_LOG_CHARS + 100));
        assert!(sanitized.ends_with("[日志已截断]"));
        assert!(sanitized.chars().count() < MAX_LOG_CHARS + 20);
    }
}
