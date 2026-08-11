use std::{
    collections::{BTreeSet, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use chrono::Local;
use reqwest::blocking::Client;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use crate::{
    models::{
        AppSettings, NodeTrafficConnection, NodeTrafficHistoryEntry, NodeTrafficPanel,
        NodeTrafficSnapshot, PortConnection, PortTrafficEntry, PortTrafficReport,
    },
    settings,
    state::AppState,
};

/// The local listener port a connection entered through, if mihomo reports it.
fn connection_inbound_port(metadata: Option<&Value>) -> Option<u16> {
    metadata_u16(metadata, "inboundPort").or_else(|| metadata_u16(metadata, "listenerPort"))
}

/// Aggregate the connections payload into per-port traffic for every enabled,
/// validly bound port slot. Keyed by `local_port`, which equals the inbound
/// listener port mihomo reports for each connection.
pub fn analyze_port_traffic(
    settings: &AppSettings,
    payload: &Value,
    running: bool,
) -> PortTrafficReport {
    use std::collections::BTreeMap;

    let mut report = PortTrafficReport {
        running,
        sampled_at: Local::now().to_rfc3339(),
        message: String::new(),
        ports: Vec::new(),
        connections: Vec::new(),
    };

    // Local ports we actually care about: enabled slots with a resolvable node.
    let mut entries: BTreeMap<u16, PortTrafficEntry> = BTreeMap::new();
    for slot in &settings.port_slots {
        if !slot.enabled {
            continue;
        }
        let Some(binding) = &slot.binding else {
            continue;
        };
        let Some(sub) = settings::find_subscription(settings, &binding.sub_id) else {
            continue;
        };
        if settings::resolve_binding(sub, binding).is_none() {
            continue;
        }
        entries
            .entry(slot.local_port)
            .or_insert_with(|| PortTrafficEntry {
                local_port: slot.local_port,
                ..PortTrafficEntry::default()
            });
    }

    if !running {
        report.message = "mihomo 未运行".to_string();
        report.ports = entries.into_values().collect();
        return report;
    }

    let connections = payload
        .get("connections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for connection in &connections {
        let metadata = connection.get("metadata");
        let Some(port) = connection_inbound_port(metadata) else {
            continue;
        };
        let Some(entry) = entries.get_mut(&port) else {
            continue;
        };
        let parsed = parse_connection(connection);
        entry.upload += parsed.upload;
        entry.download += parsed.download;
        entry.upload_speed += parsed.upload_speed;
        entry.download_speed += parsed.download_speed;
        entry.connections += 1;

        report.connections.push(PortConnection {
            id: parsed.id.clone(),
            local_port: port,
            host: if parsed.host.is_empty() {
                parsed.destination.clone()
            } else {
                parsed.host.clone()
            },
            destination: parsed.destination.clone(),
            rule: parsed.rule.clone(),
            chain: parsed.chains.last().cloned().unwrap_or_default(),
            process: parsed.process.clone(),
            network: parsed.network.clone(),
            upload: parsed.upload,
            download: parsed.download,
            upload_speed: parsed.upload_speed,
            download_speed: parsed.download_speed,
        });
    }

    report.connections.sort_by(|left, right| {
        right
            .download
            .cmp(&left.download)
            .then(right.upload.cmp(&left.upload))
    });

    report.ports = entries.into_values().collect();
    report.message = format!("已捕获 {} 条活动连接", report.connections.len());
    report
}

#[derive(Default)]
struct PortTrafficSampleState {
    sampled_at: Option<Instant>,
    previous_by_id: HashMap<String, (u64, u64)>,
}

/// Adds per-second rates to mihomo's cumulative connection counters. Mihomo's
/// connections endpoint does not consistently include `uploadSpeed` and
/// `downloadSpeed`, while the desktop UI promises live speed for every port.
#[derive(Clone, Default)]
pub struct PortTrafficSampler {
    inner: Arc<Mutex<PortTrafficSampleState>>,
}

impl PortTrafficSampler {
    pub fn sample(
        &self,
        settings: &AppSettings,
        payload: &Value,
        running: bool,
    ) -> PortTrafficReport {
        let mut report = analyze_port_traffic(settings, payload, running);
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if !running {
            state.sampled_at = None;
            state.previous_by_id.clear();
            return report;
        }

        let now = Instant::now();
        let elapsed_ms = state
            .sampled_at
            .map(|sampled_at| now.duration_since(sampled_at).as_millis() as i64)
            .unwrap_or_default();
        let mut next_previous = HashMap::new();

        for entry in &mut report.ports {
            entry.upload_speed = 0;
            entry.download_speed = 0;
        }

        for connection in &mut report.connections {
            let key = if connection.id.is_empty() {
                format!(
                    "{}|{}|{}|{}",
                    connection.local_port, connection.destination, connection.process, connection.network
                )
            } else {
                connection.id.clone()
            };
            if let Some((previous_upload, previous_download)) = state.previous_by_id.get(&key) {
                if connection.upload_speed == 0 {
                    connection.upload_speed = derive_speed(
                        connection.upload.saturating_sub(*previous_upload),
                        elapsed_ms,
                    );
                }
                if connection.download_speed == 0 {
                    connection.download_speed = derive_speed(
                        connection.download.saturating_sub(*previous_download),
                        elapsed_ms,
                    );
                }
            }
            next_previous.insert(key, (connection.upload, connection.download));

            if let Some(entry) = report
                .ports
                .iter_mut()
                .find(|entry| entry.local_port == connection.local_port)
            {
                entry.upload_speed += connection.upload_speed;
                entry.download_speed += connection.download_speed;
            }
        }

        state.sampled_at = Some(now);
        state.previous_by_id = next_previous;
        report
    }
}

/// Fetch the connections payload once and aggregate it per port.
pub fn fetch_port_traffic(
    settings: &AppSettings,
    controller_port: u16,
    controller_secret: &str,
) -> Result<PortTrafficReport> {
    let payload = fetch_port_traffic_payload(controller_port, controller_secret)?;
    Ok(analyze_port_traffic(settings, &payload, true))
}

pub fn fetch_port_traffic_payload(
    controller_port: u16,
    controller_secret: &str,
) -> Result<Value> {
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build traffic client")?;
    fetch_connections_payload(&client, controller_port, controller_secret)
}

const TRAFFIC_HISTORY_LIMIT: usize = 200;
const TRAFFIC_POLL_INTERVAL: Duration = Duration::from_millis(250);

fn value_to_u64(value: Option<&Value>) -> Option<u64> {
    match value {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text)) => text.parse::<u64>().ok(),
        _ => None,
    }
}

fn value_to_u16(value: Option<&Value>) -> Option<u16> {
    value_to_u64(value).and_then(|port| u16::try_from(port).ok())
}

fn value_to_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(flag)) => flag.to_string(),
        _ => String::new(),
    }
}

fn metadata_string(metadata: Option<&Value>, key: &str) -> String {
    metadata
        .and_then(|value| value.get(key))
        .map(|value| value_to_string(Some(value)))
        .unwrap_or_default()
}

fn metadata_u16(metadata: Option<&Value>, key: &str) -> Option<u16> {
    metadata.and_then(|value| value_to_u16(value.get(key)))
}

fn matches_local_port(metadata: Option<&Value>, local_port: u16) -> bool {
    [
        metadata_u16(metadata, "inboundPort"),
        metadata_u16(metadata, "listenerPort"),
        metadata_u16(metadata, "destinationPort"),
    ]
    .into_iter()
    .flatten()
    .any(|port| port == local_port)
}

fn extract_chains(connection: &Value) -> Vec<String> {
    connection
        .get("chains")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn matches_node(connection: &Value, node_name: &str, local_port: Option<u16>) -> bool {
    let metadata = connection.get("metadata");
    let chains = extract_chains(connection);
    let chain_match = chains.iter().any(|chain| chain == node_name)
        || value_to_string(connection.get("rule")) == node_name
        || metadata_string(metadata, "specialProxy") == node_name;

    let port_match = local_port
        .map(|port| matches_local_port(metadata, port))
        .unwrap_or(false);

    chain_match || port_match
}

fn build_destination(metadata: Option<&Value>) -> String {
    let host = metadata_string(metadata, "host");
    let destination_ip = metadata_string(metadata, "destinationIP");
    let remote_destination = metadata_string(metadata, "remoteDestination");
    let port = metadata_string(metadata, "destinationPort");

    let base = if !host.is_empty() {
        host
    } else if !remote_destination.is_empty() {
        remote_destination
    } else {
        destination_ip
    };

    if base.is_empty() {
        return String::new();
    }

    if port.is_empty() {
        base
    } else {
        format!("{base}:{port}")
    }
}

fn parse_connection(connection: &Value) -> NodeTrafficConnection {
    let metadata = connection.get("metadata");
    let host = metadata_string(metadata, "host");
    let destination = build_destination(metadata);

    NodeTrafficConnection {
        id: value_to_string(connection.get("id")),
        host: if host.is_empty() {
            destination.clone()
        } else {
            host
        },
        destination,
        rule: {
            let rule = value_to_string(connection.get("rule"));
            if rule.is_empty() {
                metadata_string(metadata, "rule")
            } else {
                rule
            }
        },
        chains: extract_chains(connection),
        upload: value_to_u64(connection.get("upload")).unwrap_or_default(),
        download: value_to_u64(connection.get("download")).unwrap_or_default(),
        upload_speed: value_to_u64(connection.get("uploadSpeed"))
            .or_else(|| value_to_u64(connection.get("upload_speed")))
            .unwrap_or_default(),
        download_speed: value_to_u64(connection.get("downloadSpeed"))
            .or_else(|| value_to_u64(connection.get("download_speed")))
            .unwrap_or_default(),
        start: value_to_string(connection.get("start")),
        process: {
            let process = metadata_string(metadata, "process");
            if process.is_empty() {
                metadata_string(metadata, "processPath")
            } else {
                process
            }
        },
        network: metadata_string(metadata, "network"),
        connection_type: metadata_string(metadata, "type"),
    }
}

fn node_key(sub_id: &str, node_index: usize) -> String {
    format!("{sub_id}:{node_index}")
}

fn active_targets(settings: &AppSettings) -> Vec<(String, usize)> {
    settings
        .subscriptions
        .iter()
        .flat_map(|sub| {
            sub.selected_node_indices
                .iter()
                .copied()
                .filter(move |index| {
                    sub.nodes.get(*index).is_some()
                        && sub.port_assignments.contains_key(&index.to_string())
                })
                .map(move |index| (sub.id.clone(), index))
        })
        .collect()
}

fn build_history_entry(
    snapshot: &NodeTrafficSnapshot,
    connection: &NodeTrafficConnection,
    upload_delta: u64,
    download_delta: u64,
    upload_speed: u64,
    download_speed: u64,
    note: String,
) -> NodeTrafficHistoryEntry {
    NodeTrafficHistoryEntry {
        id: format!("{}-{}", snapshot.sampled_at, connection.id),
        time: snapshot.sampled_at.clone(),
        host: if connection.host.is_empty() {
            connection.destination.clone()
        } else {
            connection.host.clone()
        },
        destination: connection.destination.clone(),
        rule: connection.rule.clone(),
        chain: connection.chains.last().cloned().unwrap_or_default(),
        process: connection.process.clone(),
        network: connection.network.clone(),
        upload: upload_delta,
        download: download_delta,
        upload_speed,
        download_speed,
        note,
    }
}

fn derive_speed(bytes: u64, elapsed_ms: i64) -> u64 {
    if bytes == 0 || elapsed_ms <= 0 {
        return 0;
    }
    ((bytes as u128) * 1000 / (elapsed_ms as u128)) as u64
}

fn mark_snapshot_unavailable(
    snapshot: &mut NodeTrafficSnapshot,
    running: bool,
    clear_totals: bool,
    message: &str,
) {
    snapshot.running = running;
    snapshot.sampled_at = Local::now().to_rfc3339();
    snapshot.upload_speed = 0;
    snapshot.download_speed = 0;
    if clear_totals {
        snapshot.upload_total = 0;
        snapshot.download_total = 0;
    }
    snapshot.connections.clear();
    snapshot.message = message.to_string();
}

fn update_cached_panel(cached: &mut CachedNodeTraffic, snapshot: NodeTrafficSnapshot) {
    let mut previous_by_id = HashMap::new();

    for connection in &snapshot.connections {
        let previous = cached.previous_by_id.get(&connection.id).copied();
        let mut upload_delta = previous
            .map(|(upload, _, _)| connection.upload.saturating_sub(upload))
            .unwrap_or_default();
        let mut download_delta = previous
            .map(|(_, download, _)| connection.download.saturating_sub(download))
            .unwrap_or_default();
        let mut note = if connection.rule.is_empty() {
            connection
                .chains
                .last()
                .cloned()
                .unwrap_or_else(|| "活动连接更新".to_string())
        } else {
            connection.rule.clone()
        };

        let sampled_at_ms = chrono::DateTime::parse_from_rfc3339(&snapshot.sampled_at)
            .map(|value| value.timestamp_millis())
            .unwrap_or_else(|_| Local::now().timestamp_millis());
        let elapsed_ms = if let Some((_, _, previous_sampled_at_ms)) = previous {
            (sampled_at_ms - previous_sampled_at_ms).max(TRAFFIC_POLL_INTERVAL.as_millis() as i64)
        } else {
            TRAFFIC_POLL_INTERVAL.as_millis() as i64
        };

        if previous.is_none() {
            upload_delta = connection.upload;
            download_delta = connection.download;
            note = if connection.upload_speed > 0 || connection.download_speed > 0 {
                format!("发现活动连接 · {note}")
            } else {
                format!("新连接 · {note}")
            };
        }

        let computed_upload_speed = if connection.upload_speed > 0 {
            connection.upload_speed
        } else {
            derive_speed(upload_delta, elapsed_ms)
        };
        let computed_download_speed = if connection.download_speed > 0 {
            connection.download_speed
        } else {
            derive_speed(download_delta, elapsed_ms)
        };

        if upload_delta > 0 || download_delta > 0 || previous.is_none() {
            cached.panel.session_upload += upload_delta;
            cached.panel.session_download += download_delta;
            cached.panel.total_records += 1;
            cached.panel.history.insert(
                0,
                build_history_entry(
                    &snapshot,
                    connection,
                    upload_delta,
                    download_delta,
                    computed_upload_speed,
                    computed_download_speed,
                    note,
                ),
            );
        }

        previous_by_id.insert(
            connection.id.clone(),
            (connection.upload, connection.download, sampled_at_ms),
        );
    }

    if cached.panel.history.len() > TRAFFIC_HISTORY_LIMIT {
        cached.panel.history.truncate(TRAFFIC_HISTORY_LIMIT);
    }

    cached.panel.snapshot = snapshot;
    cached.previous_by_id = previous_by_id;
}

#[derive(Clone)]
struct CachedNodeTraffic {
    panel: NodeTrafficPanel,
    previous_by_id: HashMap<String, (u64, u64, i64)>,
}

#[derive(Default)]
struct TrafficMonitorInner {
    cache: Mutex<HashMap<String, CachedNodeTraffic>>,
    started: AtomicBool,
}

#[derive(Clone, Default)]
pub struct TrafficMonitor {
    inner: Arc<TrafficMonitorInner>,
}

impl TrafficMonitor {
    pub fn start<R: Runtime>(&self, app: &AppHandle<R>) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }

        let app_handle = app.clone();
        let monitor = self.clone();
        thread::spawn(move || {
            let client = Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .ok();

            loop {
                if let Some(client) = client.as_ref() {
                    monitor.collect_once(&app_handle, client);
                }
                thread::sleep(TRAFFIC_POLL_INTERVAL);
            }
        });
    }

    pub fn panel(&self, sub_id: &str, node_index: usize) -> Option<NodeTrafficPanel> {
        self.inner
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&node_key(sub_id, node_index))
            .map(|cached| cached.panel.clone())
    }

    fn collect_once<R: Runtime>(&self, app: &AppHandle<R>, client: &Client) {
        let settings_data = match settings::load_settings() {
            Ok(settings_data) => settings_data,
            Err(_) => return,
        };

        let active_keys = active_targets(&settings_data)
            .iter()
            .map(|(sub_id, node_index)| node_key(sub_id, *node_index))
            .collect::<BTreeSet<_>>();

        let state = app.state::<AppState>();
        if !state.runner.is_running() {
            self.mark_inactive(&active_keys, "mihomo 未运行");
            return;
        }

        let Some((controller_port, controller_secret)) = state.runner.controller_access() else {
            self.mark_inactive(&active_keys, "mihomo 未运行");
            return;
        };

        let payload = match fetch_connections_payload(client, controller_port, &controller_secret) {
            Ok(payload) => payload,
            Err(_) => {
                self.mark_fetch_error(&active_keys, "连接数据获取失败，等待下一次采样");
                return;
            }
        };

        self.refresh_cache(&settings_data, &payload, &active_keys);
    }

    fn mark_inactive(&self, active_keys: &BTreeSet<String>, message: &str) {
        let mut cache = self
            .inner
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.retain(|key, _| active_keys.contains(key));
        for cached in cache.values_mut() {
            mark_snapshot_unavailable(&mut cached.panel.snapshot, false, true, message);
            cached.previous_by_id.clear();
        }
    }

    fn mark_fetch_error(&self, active_keys: &BTreeSet<String>, message: &str) {
        let mut cache = self
            .inner
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.retain(|key, _| active_keys.contains(key));
        for cached in cache.values_mut() {
            mark_snapshot_unavailable(&mut cached.panel.snapshot, true, true, message);
        }
    }

    fn refresh_cache(
        &self,
        settings: &AppSettings,
        payload: &Value,
        active_keys: &BTreeSet<String>,
    ) {
        let mut cache = self
            .inner
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.retain(|key, _| active_keys.contains(key));

        for (sub_id, node_index) in active_targets(settings) {
            let Ok(snapshot) =
                analyze_connections_payload(settings, &sub_id, node_index, payload, true)
            else {
                continue;
            };
            let key = node_key(&sub_id, node_index);
            let cached = cache.entry(key).or_insert_with(|| CachedNodeTraffic {
                panel: NodeTrafficPanel {
                    snapshot: snapshot.clone(),
                    session_upload: 0,
                    session_download: 0,
                    total_records: 0,
                    history: Vec::new(),
                },
                previous_by_id: HashMap::new(),
            });
            update_cached_panel(cached, snapshot);
        }
    }
}

pub fn analyze_connections_payload(
    settings: &AppSettings,
    sub_id: &str,
    node_index: usize,
    payload: &Value,
    running: bool,
) -> Result<NodeTrafficSnapshot> {
    let Some(sub) = settings.subscriptions.iter().find(|item| item.id == sub_id) else {
        return Err(anyhow!("目标订阅不存在"));
    };
    let Some(node) = sub.nodes.get(node_index) else {
        return Err(anyhow!("目标节点不存在"));
    };

    let local_port = sub.port_assignments.get(&node_index.to_string()).copied();
    let mut snapshot = NodeTrafficSnapshot {
        sub_id: sub.id.clone(),
        sub_name: sub.name.clone(),
        node_index,
        node_name: node.name.clone(),
        local_port,
        running,
        sampled_at: Local::now().to_rfc3339(),
        upload_total: 0,
        download_total: 0,
        upload_speed: 0,
        download_speed: 0,
        connections: Vec::new(),
        message: String::new(),
    };

    if !running {
        snapshot.message = "mihomo 未运行".to_string();
        return Ok(snapshot);
    }

    let Some(local_port) = local_port else {
        snapshot.message = "当前节点未启用，尚未分配本地监听端口".to_string();
        return Ok(snapshot);
    };

    let connections = payload
        .get("connections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for connection in connections {
        if !matches_node(&connection, &node.name, Some(local_port)) {
            continue;
        }
        let parsed = parse_connection(&connection);
        snapshot.upload_total += parsed.upload;
        snapshot.download_total += parsed.download;
        snapshot.upload_speed += parsed.upload_speed;
        snapshot.download_speed += parsed.download_speed;
        snapshot.connections.push(parsed);
    }

    snapshot.connections.sort_by(|left, right| {
        right
            .download
            .cmp(&left.download)
            .then(right.upload.cmp(&left.upload))
    });

    if snapshot.connections.is_empty() {
        snapshot.message = "当前节点暂无活动连接".to_string();
    } else {
        snapshot.message = format!("已捕获 {} 条活动连接", snapshot.connections.len());
    }

    Ok(snapshot)
}

pub fn fetch_connections_payload(
    client: &Client,
    controller_port: u16,
    controller_secret: &str,
) -> Result<Value> {
    client
        .get(format!("http://127.0.0.1:{controller_port}/connections"))
        .bearer_auth(controller_secret)
        .send()
        .context("request connections")?
        .error_for_status()
        .context("controller returned error")?
        .json::<Value>()
        .context("parse connections payload")
}

pub fn fetch_node_traffic_snapshot(
    settings: &AppSettings,
    controller_port: u16,
    controller_secret: &str,
    sub_id: &str,
    node_index: usize,
) -> Result<NodeTrafficSnapshot> {
    let client = Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("build traffic client")?;

    let payload = fetch_connections_payload(&client, controller_port, controller_secret)?;

    analyze_connections_payload(settings, sub_id, node_index, &payload, true)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;
    use crate::models::{AppSettings, NodeTrafficPanel, ProxyNode, SubscriptionRecord};

    fn sample_settings() -> AppSettings {
        AppSettings {
            schema_version: 3,
            subscriptions: vec![SubscriptionRecord {
                id: "sub-a".to_string(),
                name: "订阅A".to_string(),
                url: String::new(),
                ua: "clash.meta".to_string(),
                start_port: 10801,
                manual: false,
                content: String::new(),
                nodes: vec![ProxyNode {
                    name: "Japan | 02".to_string(),
                    node_type: "trojan".to_string(),
                    server: "jp.example.com".to_string(),
                    port: 443,
                    ..ProxyNode::default()
                }],
                selected_node_indices: vec![0],
                port_assignments: [("0".to_string(), 10901)].into_iter().collect(),
                node_remarks: Default::default(),
            }],
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
            mihomo_path: crate::models::default_mihomo_path_string(),
        }
    }

    #[test]
    fn analyze_connections_matches_by_chain() {
        let settings = sample_settings();
        let payload = json!({
            "connections": [
                {
                    "id": "conn-1",
                    "upload": 1200,
                    "download": 3400,
                    "uploadSpeed": 12,
                    "downloadSpeed": 34,
                    "chains": ["节点选择", "Japan | 02"],
                    "rule": "GeoLocation-!CN",
                    "start": "2026-05-02T10:00:00Z",
                    "metadata": {
                        "host": "graph.microsoft.com",
                        "destinationIP": "13.107.42.16",
                        "destinationPort": "443",
                        "network": "tcp",
                        "type": "mixed",
                        "process": "Code.exe"
                    }
                }
            ]
        });

        let result = analyze_connections_payload(&settings, "sub-a", 0, &payload, true)
            .expect("analyze connections");

        assert_eq!(result.connections.len(), 1);
        assert_eq!(result.upload_total, 1200);
        assert_eq!(result.download_total, 3400);
        assert_eq!(result.connections[0].host, "graph.microsoft.com");
    }

    #[test]
    fn analyze_connections_matches_by_listener_port() {
        let settings = sample_settings();
        let payload = json!({
            "connections": [
                {
                    "id": "conn-2",
                    "upload": 10,
                    "download": 20,
                    "uploadSpeed": 1,
                    "downloadSpeed": 2,
                    "chains": [],
                    "start": "2026-05-02T10:00:00Z",
                    "metadata": {
                        "host": "login.microsoftonline.com",
                        "destinationPort": 10901,
                        "network": "tcp",
                        "type": "mixed"
                    }
                }
            ]
        });

        let result = analyze_connections_payload(&settings, "sub-a", 0, &payload, true)
            .expect("analyze connections");

        assert_eq!(result.connections.len(), 1);
        assert_eq!(
            result.connections[0].destination,
            "login.microsoftonline.com:10901"
        );
    }

    #[test]
    fn port_sampler_derives_live_speed_from_connection_deltas() {
        let mut settings = sample_settings();
        let node = settings.subscriptions[0].nodes[0].clone();
        settings.port_slots.push(crate::models::PortSlot {
            id: "slot-a".to_string(),
            name: "slot-a".to_string(),
            note: String::new(),
            local_port: 10901,
            enabled: true,
            binding: Some(crate::models::NodeBinding {
                sub_id: "sub-a".to_string(),
                fingerprint: settings::node_fingerprint(&node),
                node_name: node.name,
            }),
        });
        let sampler = PortTrafficSampler::default();
        let first = json!({
            "connections": [{
                "id": "conn-speed",
                "upload": 1000,
                "download": 2000,
                "metadata": { "inboundPort": 10901, "host": "example.com" }
            }]
        });
        let second = json!({
            "connections": [{
                "id": "conn-speed",
                "upload": 3000,
                "download": 7000,
                "metadata": { "inboundPort": 10901, "host": "example.com" }
            }]
        });

        let initial = sampler.sample(&settings, &first, true);
        assert_eq!(initial.ports[0].upload_speed, 0);
        std::thread::sleep(Duration::from_millis(10));
        let sampled = sampler.sample(&settings, &second, true);

        assert!(sampled.connections[0].upload_speed > 0);
        assert!(sampled.connections[0].download_speed > 0);
        assert_eq!(sampled.ports[0].upload_speed, sampled.connections[0].upload_speed);
        assert_eq!(sampled.ports[0].download_speed, sampled.connections[0].download_speed);
    }

    #[test]
    fn first_seen_connection_counts_current_totals_into_history_and_session() {
        let settings = sample_settings();
        let payload = json!({
            "connections": [
                {
                    "id": "conn-3",
                    "upload": 2048,
                    "download": 8192,
                    "uploadSpeed": 0,
                    "downloadSpeed": 0,
                    "chains": ["节点选择", "Japan | 02"],
                    "rule": "GeoLocation-!CN",
                    "start": "2026-05-02T10:00:00Z",
                    "metadata": {
                        "host": "ipinfo.io",
                        "destinationPort": "443",
                        "network": "tcp",
                        "type": "mixed",
                        "process": "chrome.exe"
                    }
                }
            ]
        });

        let snapshot = analyze_connections_payload(&settings, "sub-a", 0, &payload, true)
            .expect("analyze connections");
        let mut cached = CachedNodeTraffic {
            panel: NodeTrafficPanel {
                snapshot: snapshot.clone(),
                session_upload: 0,
                session_download: 0,
                total_records: 0,
                history: Vec::new(),
            },
            previous_by_id: HashMap::new(),
        };

        update_cached_panel(&mut cached, snapshot);

        assert_eq!(cached.panel.session_upload, 2048);
        assert_eq!(cached.panel.session_download, 8192);
        assert_eq!(cached.panel.total_records, 1);
        assert_eq!(cached.panel.history.len(), 1);
        assert_eq!(cached.panel.history[0].upload, 2048);
        assert_eq!(cached.panel.history[0].download, 8192);
        assert!(cached.panel.history[0].upload_speed > 0);
        assert!(cached.panel.history[0].download_speed > 0);
    }

    #[test]
    fn unavailable_snapshot_clears_live_metrics() {
        let mut snapshot = NodeTrafficSnapshot {
            sub_id: "sub-a".to_string(),
            sub_name: "订阅A".to_string(),
            node_index: 0,
            node_name: "Japan | 02".to_string(),
            local_port: Some(10901),
            running: true,
            sampled_at: "2026-05-02T10:00:00Z".to_string(),
            upload_total: 1200,
            download_total: 3400,
            upload_speed: 12,
            download_speed: 34,
            connections: vec![NodeTrafficConnection {
                id: "conn-1".to_string(),
                host: "graph.microsoft.com".to_string(),
                destination: "graph.microsoft.com:443".to_string(),
                rule: "GeoLocation-!CN".to_string(),
                chains: vec!["节点选择".to_string(), "Japan | 02".to_string()],
                upload: 1200,
                download: 3400,
                upload_speed: 12,
                download_speed: 34,
                start: "2026-05-02T10:00:00Z".to_string(),
                process: "Code.exe".to_string(),
                network: "tcp".to_string(),
                connection_type: "mixed".to_string(),
            }],
            message: "已捕获 1 条活动连接".to_string(),
        };

        mark_snapshot_unavailable(
            &mut snapshot,
            true,
            true,
            "连接数据获取失败，等待下一次采样",
        );

        assert!(snapshot.running);
        assert_eq!(snapshot.upload_total, 0);
        assert_eq!(snapshot.download_total, 0);
        assert_eq!(snapshot.upload_speed, 0);
        assert_eq!(snapshot.download_speed, 0);
        assert!(snapshot.connections.is_empty());
        assert_eq!(snapshot.message, "连接数据获取失败，等待下一次采样");
    }
}
