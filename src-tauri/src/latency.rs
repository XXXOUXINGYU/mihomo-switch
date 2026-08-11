use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    config,
    logging::{emit_latency_result, emit_log},
    models::{AppSettings, LatencyResult, ProxyNode},
    runtime_paths, settings,
    state::LatencyControl,
};

const TEST_URL: &str = "http://www.gstatic.com/generate_204";

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

struct TempConfigGuard(std::path::PathBuf);

impl Drop for TempConfigGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn allocate_local_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("bind ephemeral port")?;
    Ok(listener.local_addr().context("read local address")?.port())
}

fn build_controller_client() -> Result<Client> {
    Client::builder().no_proxy().build().map_err(Into::into)
}

fn build_latency_config(
    nodes: &[(usize, ProxyNode)],
    mixed_port: u16,
    controller_port: u16,
    controller_secret: &str,
) -> Value {
    let mut proxies = Vec::with_capacity(nodes.len());
    let mut proxy_names = Vec::with_capacity(nodes.len());

    for (position, (_, node)) in nodes.iter().enumerate() {
        let proxy_name = format!("LATENCY-{position}");
        let mut proxy = config::build_proxy_value(node);
        if let Some(object) = proxy.as_object_mut() {
            object.insert("name".into(), Value::String(proxy_name.clone()));
        }
        proxies.push(proxy);
        proxy_names.push(proxy_name);
    }

    json!({
        "mixed-port": mixed_port,
        "allow-lan": false,
        "mode": "rule",
        "log-level": "warning",
        "external-controller": format!("127.0.0.1:{controller_port}"),
        "secret": controller_secret,
        "proxies": proxies,
        "proxy-groups": [{
            "name": "LATENCY-TEST",
            "type": "select",
            "proxies": proxy_names,
        }],
        "rules": ["MATCH,LATENCY-TEST"],
    })
}

async fn wait_controller_ready(
    client: &Client,
    controller_port: u16,
    controller_secret: &str,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let version_url = format!("http://127.0.0.1:{controller_port}/version");
    let mut last_error = String::new();

    while Instant::now() < deadline {
        match client
            .get(&version_url)
            .bearer_auth(controller_secret)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => {
                last_error = format!("status={}", response.status());
            }
            Err(error) => {
                last_error = error.to_string();
            }
        }
        thread::sleep(Duration::from_millis(200));
    }

    Err(anyhow!("mihomo controller 启动超时: {last_error}"))
}

async fn probe_delay(
    client: &Client,
    controller_port: u16,
    controller_secret: &str,
    proxy_name: &str,
    timeout_ms: u64,
) -> String {
    let encoded_name = urlencoding::encode(proxy_name);
    let request = client
        .get(format!(
            "http://127.0.0.1:{controller_port}/proxies/{encoded_name}/delay"
        ))
        .bearer_auth(controller_secret)
        .query(&[
            ("timeout", timeout_ms.to_string()),
            ("url", TEST_URL.to_string()),
        ])
        .timeout(Duration::from_millis(timeout_ms.saturating_add(2_000)));

    match request.send().await {
        Ok(response) if response.status().is_success() => match response.json::<Value>().await {
            Ok(value) => value
                .get("delay")
                .and_then(Value::as_i64)
                .map(|delay| format!("{delay}ms"))
                .unwrap_or_else(|| "失败".to_string()),
            Err(_) => "失败".to_string(),
        },
        Ok(_) => "失败".to_string(),
        Err(error) if error.is_timeout() => "超时".to_string(),
        Err(_) => "失败".to_string(),
    }
}

fn spawn_latency_mihomo(settings: &AppSettings) -> Result<Child> {
    let mihomo = runtime_paths::resolve_mihomo_path(&settings.mihomo_path);
    if !mihomo.exists() {
        return Err(anyhow!("mihomo.exe 不存在: {}", mihomo.display()));
    }

    let config_path = runtime_paths::latency_config_path()?;
    let mut command = Command::new(&mihomo);
    command
        .arg("-f")
        .arg(&config_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    command
        .spawn()
        .with_context(|| format!("spawn latency mihomo: {}", mihomo.display()))
}

pub async fn test_nodes_latency(
    app: &tauri::AppHandle,
    sub_id: &str,
    nodes: &[(usize, ProxyNode)],
    timeout_ms: u64,
    control: &LatencyControl,
) -> Result<Vec<LatencyResult>> {
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    let run = control
        .try_begin_run()
        .ok_or_else(|| anyhow!("已有测速任务正在进行，请等待完成或先取消"))?;
    let run_token = run.token();

    let mixed_port = allocate_local_port()?;
    let controller_port = allocate_local_port()?;
    let controller_secret = Uuid::new_v4().to_string();
    let config_path = runtime_paths::latency_config_path()?;
    let config_value = build_latency_config(nodes, mixed_port, controller_port, &controller_secret);
    let config_yaml = serde_yaml::to_string(&config_value)?;
    std::fs::write(&config_path, config_yaml)
        .with_context(|| format!("write latency config: {}", config_path.display()))?;
    let _config = TempConfigGuard(config_path);

    let settings_data = settings::load_settings()?;
    let client = build_controller_client()?;
    let _child = ChildGuard(spawn_latency_mihomo(&settings_data)?);

    emit_log(
        app,
        "info",
        format!("开始测速: {} 个节点（并发 1）", nodes.len()),
    );
    let result = async {
        wait_controller_ready(&client, controller_port, &controller_secret).await?;
        let mut results = Vec::with_capacity(nodes.len());

        for (position, (node_index, node)) in nodes.iter().enumerate() {
            if !control.is_active(run_token) {
                emit_log(app, "info", "测速任务已取消");
                break;
            }
            let proxy_name = format!("LATENCY-{position}");
            let delay = probe_delay(
                &client,
                controller_port,
                &controller_secret,
                &proxy_name,
                timeout_ms,
            )
            .await;
            if !control.is_active(run_token) {
                emit_log(app, "info", "测速任务已取消");
                break;
            }
            emit_log(app, "info", format!("测速完成: {} -> {}", node.name, delay));
            let result = LatencyResult {
                sub_id: sub_id.to_string(),
                node_index: *node_index,
                result: delay,
            };
            emit_latency_result(app, &result);
            results.push(result);
        }

        if control.is_active(run_token) {
            emit_log(app, "info", "测速完成");
        }
        Ok(results)
    }
    .await;

    result
}

#[allow(dead_code)]
pub async fn test_nodes_latency_headless(
    sub_id: &str,
    nodes: &[(usize, ProxyNode)],
    timeout_ms: u64,
) -> Result<Vec<LatencyResult>> {
    if nodes.is_empty() {
        return Ok(Vec::new());
    }

    let mixed_port = allocate_local_port()?;
    let controller_port = allocate_local_port()?;
    let controller_secret = Uuid::new_v4().to_string();
    let config_path = runtime_paths::latency_config_path()?;
    let config_value = build_latency_config(nodes, mixed_port, controller_port, &controller_secret);
    let config_yaml = serde_yaml::to_string(&config_value)?;
    std::fs::write(&config_path, config_yaml)
        .with_context(|| format!("write latency config: {}", config_path.display()))?;
    let _config = TempConfigGuard(config_path);

    let settings_data = settings::load_settings()?;
    let client = build_controller_client()?;
    let _child = ChildGuard(spawn_latency_mihomo(&settings_data)?);

    let result = async {
        wait_controller_ready(&client, controller_port, &controller_secret).await?;
        let mut results = Vec::with_capacity(nodes.len());

        for (position, (node_index, _node)) in nodes.iter().enumerate() {
            let proxy_name = format!("LATENCY-{position}");
            let delay = probe_delay(
                &client,
                controller_port,
                &controller_secret,
                &proxy_name,
                timeout_ms,
            )
            .await;
            results.push(LatencyResult {
                sub_id: sub_id.to_string(),
                node_index: *node_index,
                result: delay,
            });
        }

        Ok(results)
    }
    .await;

    result
}
