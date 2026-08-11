use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RealityOptions {
    #[serde(
        rename = "public-key",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub public_key: String,
    #[serde(rename = "short-id", default, skip_serializing_if = "String::is_empty")]
    pub short_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyNode {
    pub name: String,
    #[serde(rename = "type", default)]
    pub node_type: String,
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uuid: String,
    #[serde(default)]
    pub alter_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cipher: String,
    #[serde(default = "default_true")]
    pub udp: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub flow: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub encryption: String,
    #[serde(default)]
    pub tls: bool,
    #[serde(rename = "skip-cert-verify", default)]
    pub skip_cert_verify: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub servername: String,
    #[serde(
        rename = "reality-opts",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub reality_opts: Option<RealityOptions>,
    #[serde(
        rename = "client-fingerprint",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub client_fingerprint: String,
    #[serde(default = "default_network", skip_serializing_if = "String::is_empty")]
    pub network: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_ua")]
    pub ua: String,
    #[serde(default = "default_start_port")]
    pub start_port: u16,
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub nodes: Vec<ProxyNode>,
    #[serde(default)]
    pub selected_node_indices: Vec<usize>,
    #[serde(default)]
    pub port_assignments: BTreeMap<String, u16>,
    #[serde(default)]
    pub node_remarks: BTreeMap<String, String>,
}

/// Stable, identity-based pointer from a port slot to a subscription node.
///
/// We deliberately bind by a transport fingerprint instead of a node index so a
/// binding survives subscription updates that reorder or re-import nodes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeBinding {
    pub sub_id: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub node_name: String,
}

/// A fixed local proxy port owned by the user. The port number is stable and
/// independent of which node is currently bound to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSlot {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub note: String,
    pub local_port: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub binding: Option<NodeBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub schema_version: u8,
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionRecord>,
    #[serde(default)]
    pub port_slots: Vec<PortSlot>,
    #[serde(default)]
    pub slots_migrated: bool,
    #[serde(default)]
    pub subconverter: String,
    #[serde(default)]
    pub local_proxy_enabled: bool,
    #[serde(default = "default_local_proxy_url")]
    pub local_proxy_url: String,
    #[serde(default = "default_mihomo_path_string")]
    pub mihomo_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSnapshot {
    pub config_path: String,
    pub mihomo_path: String,
    pub mihomo_exists: bool,
    pub runtime_dir: String,
    pub running: bool,
}

/// Resolved, render-ready view of a port slot. Computed on every bootstrap and
/// never persisted: it carries the current resolution of the binding against
/// the live node list so the UI can show "valid" / "invalid" / "unbound".
#[derive(Debug, Clone, Serialize)]
pub struct PortSlotBindingView {
    pub sub_id: String,
    pub sub_name: String,
    pub node_index: Option<usize>,
    pub node_name: String,
    pub node_type: String,
    pub server: String,
    pub server_port: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortSlotView {
    pub id: String,
    pub name: String,
    pub note: String,
    pub local_port: u16,
    pub enabled: bool,
    /// One of: "unbound" | "valid" | "invalid".
    pub state: String,
    pub invalid_reason: Option<String>,
    pub binding: Option<PortSlotBindingView>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MigrationReport {
    pub migrated: bool,
    pub created_slots: usize,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortSlotBindingInput {
    pub sub_id: String,
    pub node_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortSlotBatchBindingInput {
    pub slot_id: String,
    pub sub_id: String,
    pub node_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSlotInput {
    #[serde(default)]
    pub name: String,
    pub local_port: u16,
    #[serde(default)]
    pub note: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub binding: Option<PortSlotBindingInput>,
}

/// Result of validating a candidate local port number.
#[derive(Debug, Clone, Serialize)]
pub struct PortValidation {
    /// One of: "ok" | "invalid" | "conflict" | "occupied".
    pub status: String,
    pub message: String,
}

/// Aggregated live traffic for a single local port (one port slot).
#[derive(Debug, Clone, Serialize, Default)]
pub struct PortTrafficEntry {
    pub local_port: u16,
    pub upload: u64,
    pub download: u64,
    pub upload_speed: u64,
    pub download_speed: u64,
    pub connections: u32,
}

/// A single live connection attributed to a local port.
#[derive(Debug, Clone, Serialize)]
pub struct PortConnection {
    pub id: String,
    pub local_port: u16,
    pub host: String,
    pub destination: String,
    pub rule: String,
    pub chain: String,
    pub process: String,
    pub network: String,
    pub upload: u64,
    pub download: u64,
    pub upload_speed: u64,
    pub download_speed: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PortTrafficReport {
    pub running: bool,
    pub sampled_at: String,
    pub message: String,
    pub ports: Vec<PortTrafficEntry>,
    pub connections: Vec<PortConnection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapPayload {
    pub settings: AppSettings,
    pub slots: Vec<PortSlotView>,
    pub runtime: RuntimeSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSubscriptionInput {
    pub name: String,
    #[serde(default)]
    pub url: String,
    pub manual: bool,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyResult {
    pub sub_id: String,
    pub node_index: usize,
    pub result: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeTrafficConnection {
    pub id: String,
    pub host: String,
    pub destination: String,
    pub rule: String,
    pub chains: Vec<String>,
    pub upload: u64,
    pub download: u64,
    pub upload_speed: u64,
    pub download_speed: u64,
    pub start: String,
    pub process: String,
    pub network: String,
    #[serde(rename = "type")]
    pub connection_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeTrafficSnapshot {
    pub sub_id: String,
    pub sub_name: String,
    pub node_index: usize,
    pub node_name: String,
    pub local_port: Option<u16>,
    pub running: bool,
    pub sampled_at: String,
    pub upload_total: u64,
    pub download_total: u64,
    pub upload_speed: u64,
    pub download_speed: u64,
    pub connections: Vec<NodeTrafficConnection>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeTrafficHistoryEntry {
    pub id: String,
    pub time: String,
    pub host: String,
    pub destination: String,
    pub rule: String,
    pub chain: String,
    pub process: String,
    pub network: String,
    pub upload: u64,
    pub download: u64,
    pub upload_speed: u64,
    pub download_speed: u64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeTrafficPanel {
    pub snapshot: NodeTrafficSnapshot,
    pub session_upload: u64,
    pub session_download: u64,
    pub total_records: u64,
    pub history: Vec<NodeTrafficHistoryEntry>,
}

pub fn default_true() -> bool {
    true
}

pub fn default_network() -> String {
    "tcp".to_string()
}

pub fn default_ua() -> String {
    "clash-verge/2.2.3".to_string()
}

pub fn default_start_port() -> u16 {
    10801
}

pub fn default_local_proxy_url() -> String {
    "http://127.0.0.1:20122".to_string()
}

pub fn default_mihomo_path_string() -> String {
    crate::runtime_paths::default_mihomo_path()
        .display()
        .to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 4,
            subscriptions: Vec::new(),
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            local_proxy_enabled: false,
            local_proxy_url: default_local_proxy_url(),
            mihomo_path: default_mihomo_path_string(),
        }
    }
}
