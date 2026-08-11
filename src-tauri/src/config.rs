use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    models::{AppSettings, ProxyNode},
    runtime_paths, settings,
};

/// One runnable port slot resolved to its live node: an enabled slot with a
/// valid binding that still points to an existing node.
pub struct ActiveSlot {
    pub node: ProxyNode,
    pub local_port: u16,
    pub slot_name: String,
}

/// Collect every port slot that is currently runnable.
pub fn collect_active_slots(settings: &AppSettings) -> Vec<ActiveSlot> {
    let mut active = Vec::new();
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
        let Some((_, node)) = settings::resolve_binding(sub, binding) else {
            continue;
        };
        active.push(ActiveSlot {
            node: node.clone(),
            local_port: slot.local_port,
            slot_name: slot.name.clone(),
        });
    }
    active
}

/// Write the mihomo config from the port slots. Multiple slots may bind the
/// same node, in which case one proxy is emitted and each slot gets its own
/// listener pointing at it.
pub struct PreparedPoolConfig {
    canonical_path: PathBuf,
    temp_path: Option<PathBuf>,
}

impl PreparedPoolConfig {
    pub fn path(&self) -> Option<&Path> {
        self.temp_path.as_deref()
    }

    pub fn commit(mut self) -> Result<Option<String>> {
        let Some(temp_path) = self.temp_path.take() else {
            if self.canonical_path.exists() {
                fs::remove_file(&self.canonical_path)
                    .with_context(|| format!("remove config: {}", self.canonical_path.display()))?;
            }
            return Ok(None);
        };

        let backup_path = self
            .canonical_path
            .with_file_name(format!("pool_config.{}.yaml.bak", Uuid::new_v4()));
        let had_canonical = self.canonical_path.exists();
        if had_canonical {
            fs::rename(&self.canonical_path, &backup_path).with_context(|| {
                format!("stage existing config: {}", self.canonical_path.display())
            })?;
        }
        if let Err(error) = fs::rename(&temp_path, &self.canonical_path) {
            let _ = fs::remove_file(&temp_path);
            if had_canonical {
                let _ = fs::rename(&backup_path, &self.canonical_path);
            }
            return Err(error)
                .with_context(|| format!("replace config: {}", self.canonical_path.display()));
        }
        if had_canonical {
            let _ = fs::remove_file(backup_path);
        }
        Ok(Some(self.canonical_path.display().to_string()))
    }
}

impl Drop for PreparedPoolConfig {
    fn drop(&mut self) {
        if let Some(path) = self.temp_path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

pub fn prepare_pool_config(settings: &AppSettings) -> Result<PreparedPoolConfig> {
    let active = collect_active_slots(settings);
    let path = runtime_paths::pool_config_path()?;
    if active.is_empty() {
        return Ok(PreparedPoolConfig {
            canonical_path: path,
            temp_path: None,
        });
    }

    let mut proxy_names = BTreeSet::new();
    let mut listener_names = BTreeSet::new();
    let mut listener_ports = BTreeSet::new();
    let mut identity_to_proxy: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut proxies = Vec::new();
    let mut listeners = Vec::new();

    for active_slot in active {
        if !listener_ports.insert(active_slot.local_port) {
            return Err(anyhow!(
                "端口 {} 被多个已启用槽位重复使用",
                active_slot.local_port
            ));
        }
        let node = &active_slot.node;
        let identity = format!("{}\u{0}{}", settings::node_fingerprint(node), node.name);
        let proxy_name = match identity_to_proxy.get(&identity) {
            Some(name) => name.clone(),
            None => {
                let base = if node.name.trim().is_empty() {
                    "node".to_string()
                } else {
                    node.name.clone()
                };
                let mut candidate = base.clone();
                let mut index = 2;
                while proxy_names.contains(&candidate) {
                    candidate = format!("{base}-{index}");
                    index += 1;
                }
                proxy_names.insert(candidate.clone());
                let mut value = build_proxy_value(node);
                if let Some(object) = value.as_object_mut() {
                    object.insert("name".into(), Value::String(candidate.clone()));
                }
                proxies.push(value);
                identity_to_proxy.insert(identity, candidate.clone());
                candidate
            }
        };

        let base_label = if active_slot.slot_name.trim().is_empty() {
            node.name.as_str()
        } else {
            active_slot.slot_name.as_str()
        };
        let listener_name = make_unique_listener_name(base_label, &mut listener_names);
        listeners.push(json!({
            "name": listener_name,
            "type": "mixed",
            "listen": "127.0.0.1",
            "port": active_slot.local_port,
            "proxy": proxy_name,
        }));
    }

    let config = json!({
        "proxies": proxies,
        "listeners": listeners,
    });
    let yaml = serde_yaml::to_string(&config)?;
    let temp_path = path.with_file_name(format!("pool_config.{}.yaml.tmp", Uuid::new_v4()));
    fs::write(&temp_path, &yaml)
        .with_context(|| format!("write temp config: {}", temp_path.display()))?;
    Ok(PreparedPoolConfig {
        canonical_path: path,
        temp_path: Some(temp_path),
    })
}

pub fn write_pool_config(settings: &AppSettings) -> Result<Option<String>> {
    prepare_pool_config(settings)?.commit()
}

pub(crate) fn build_proxy_value(node: &ProxyNode) -> Value {
    if let Some(raw) = &node.raw {
        if raw.get("type").is_some() {
            let mut value = raw.clone();
            if let Some(object) = value.as_object_mut() {
                object.insert("name".into(), Value::String(node.name.clone()));
                if !object.contains_key("tls")
                    && matches!(node.node_type.as_str(), "trojan" | "vless")
                {
                    object.insert("tls".into(), Value::Bool(true));
                }
                if let Some(sni) = object.remove("sni") {
                    if !object.contains_key("servername") {
                        object.insert("servername".into(), sni);
                    }
                }
            }
            return prune(value);
        }
    }

    let mut map = Map::<String, Value>::new();
    map.insert("name".into(), Value::String(node.name.clone()));
    map.insert("type".into(), Value::String(node.node_type.clone()));
    map.insert("server".into(), Value::String(node.server.clone()));
    map.insert("port".into(), json!(node.port));

    match node.node_type.as_str() {
        "vless" => {
            map.insert("uuid".into(), Value::String(node.uuid.clone()));
            map.insert("alterId".into(), json!(node.alter_id));
            map.insert("cipher".into(), Value::String(node.cipher.clone()));
            map.insert("udp".into(), Value::Bool(node.udp));
            map.insert("flow".into(), Value::String(node.flow.clone()));
            map.insert("encryption".into(), Value::String(node.encryption.clone()));
            map.insert("tls".into(), Value::Bool(node.tls));
            map.insert(
                "skip-cert-verify".into(),
                Value::Bool(node.skip_cert_verify),
            );
            map.insert("servername".into(), Value::String(node.servername.clone()));
            map.insert(
                "client-fingerprint".into(),
                Value::String(node.client_fingerprint.clone()),
            );
            map.insert("network".into(), Value::String(node.network.clone()));
            if let Some(reality) = &node.reality_opts {
                map.insert(
                    "reality-opts".into(),
                    json!({
                        "public-key": reality.public_key,
                        "short-id": reality.short_id,
                    }),
                );
            }
        }
        "vmess" => {
            map.insert("uuid".into(), Value::String(node.uuid.clone()));
            map.insert("alterId".into(), json!(node.alter_id));
            map.insert("cipher".into(), Value::String(node.cipher.clone()));
            map.insert("udp".into(), Value::Bool(true));
            map.insert("tls".into(), Value::Bool(node.tls));
            map.insert(
                "skip-cert-verify".into(),
                Value::Bool(node.skip_cert_verify),
            );
            map.insert("servername".into(), Value::String(node.servername.clone()));
            map.insert(
                "client-fingerprint".into(),
                Value::String(node.client_fingerprint.clone()),
            );
            map.insert("network".into(), Value::String(node.network.clone()));
        }
        "trojan" => {
            map.insert("password".into(), Value::String(node.uuid.clone()));
            map.insert("udp".into(), Value::Bool(true));
            map.insert("tls".into(), Value::Bool(node.tls));
            map.insert(
                "skip-cert-verify".into(),
                Value::Bool(node.skip_cert_verify),
            );
            map.insert("servername".into(), Value::String(node.servername.clone()));
            map.insert(
                "client-fingerprint".into(),
                Value::String(node.client_fingerprint.clone()),
            );
            map.insert("network".into(), Value::String(node.network.clone()));
        }
        "ss" => {
            map.insert("password".into(), Value::String(node.uuid.clone()));
            map.insert("cipher".into(), Value::String(node.cipher.clone()));
            map.insert("udp".into(), Value::Bool(true));
        }
        _ => {}
    }

    prune(Value::Object(map))
}

fn prune(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut next = Map::new();
            for (key, value) in map {
                let pruned = prune(value);
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
        Value::Array(values) => Value::Array(values.into_iter().map(prune).collect()),
        other => other,
    }
}

fn sanitize_listener_name(name: &str) -> String {
    let ascii = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = ascii
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        "node".to_string()
    } else {
        compact
    }
}

fn make_unique_listener_name(name: &str, used: &mut BTreeSet<String>) -> String {
    let base = format!("listener-{}", sanitize_listener_name(name));
    let mut candidate = base.clone();
    let mut index = 2;
    while used.contains(&candidate) {
        candidate = format!("{base}-{index}");
        index += 1;
    }
    used.insert(candidate.clone());
    candidate
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::Path};

    use super::*;

    fn sample_node(name: &str, server: &str, port: u16) -> ProxyNode {
        ProxyNode {
            name: name.to_string(),
            node_type: "trojan".to_string(),
            server: server.to_string(),
            port,
            uuid: "secret".to_string(),
            tls: true,
            udp: true,
            ..ProxyNode::default()
        }
    }

    fn settings_with_slots(
        nodes: Vec<ProxyNode>,
        slots: Vec<crate::models::PortSlot>,
    ) -> AppSettings {
        AppSettings {
            schema_version: 4,
            subscriptions: vec![crate::models::SubscriptionRecord {
                id: "sub-a".to_string(),
                name: "sub-a".to_string(),
                url: String::new(),
                ua: "clash.meta".to_string(),
                start_port: 10801,
                manual: false,
                content: String::new(),
                nodes,
                selected_node_indices: Vec::new(),
                port_assignments: std::collections::BTreeMap::new(),
                node_remarks: std::collections::BTreeMap::new(),
            }],
            port_slots: slots,
            slots_migrated: true,
            subconverter: String::new(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
            mihomo_path: crate::models::default_mihomo_path_string(),
        }
    }

    fn slot(name: &str, port: u16, node: &ProxyNode, enabled: bool) -> crate::models::PortSlot {
        crate::models::PortSlot {
            id: format!("slot-{name}-{port}"),
            name: name.to_string(),
            note: String::new(),
            local_port: port,
            enabled,
            binding: Some(crate::models::NodeBinding {
                sub_id: "sub-a".to_string(),
                fingerprint: crate::settings::node_fingerprint(node),
                node_name: node.name.clone(),
            }),
        }
    }

    #[test]
    fn write_pool_config_creates_unique_listener_names() {
        let _guard = crate::runtime_paths::test_env_lock()
            .lock()
            .expect("env lock");
        let temp_root =
            env::temp_dir().join(format!("mihomo-switch-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).expect("create temp runtime");
        env::set_var("MIHOMO_MANAGER_HOME", &temp_root);

        let node_a = sample_node("dup node", "one.example.com", 443);
        let node_b = sample_node("dup node", "two.example.com", 8443);
        let settings = settings_with_slots(
            vec![node_a.clone(), node_b.clone()],
            vec![
                slot("dup node", 10801, &node_a, true),
                slot("dup node", 10802, &node_b, true),
            ],
        );
        let result = write_pool_config(&settings).expect("write pool config");
        let config_path = result.expect("config path");
        let config_text = fs::read_to_string(&config_path).expect("read config");

        assert!(config_text.contains("listener-dup-node"));
        assert!(config_text.contains("listener-dup-node-2"));
        let config: Value = serde_yaml::from_str(&config_text).expect("parse generated config");
        let listeners = config["listeners"].as_array().expect("listeners array");
        assert!(
            listeners
                .iter()
                .all(|listener| listener["listen"] == "127.0.0.1"),
            "every proxy listener must be restricted to localhost"
        );

        env::remove_var("MIHOMO_MANAGER_HOME");
        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn write_pool_config_removes_file_when_no_slots_active() {
        let _guard = crate::runtime_paths::test_env_lock()
            .lock()
            .expect("env lock");
        let temp_root = env::temp_dir().join(format!("mihomo-switch-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).expect("create temp runtime");
        env::set_var("MIHOMO_MANAGER_HOME", &temp_root);

        let node_a = sample_node("node", "one.example.com", 443);
        let populated = write_pool_config(&settings_with_slots(
            vec![node_a.clone()],
            vec![slot("node", 10801, &node_a, true)],
        ))
        .expect("write populated config");
        let config_path = populated.expect("config path");
        assert!(Path::new(&config_path).exists());

        let empty = write_pool_config(&settings_with_slots(
            vec![node_a.clone()],
            vec![slot("node", 10801, &node_a, false)],
        ))
        .expect("write empty config");
        assert!(empty.is_none());
        assert!(!Path::new(&config_path).exists());

        env::remove_var("MIHOMO_MANAGER_HOME");
        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn prepared_pool_config_preserves_canonical_until_commit() {
        let _guard = crate::runtime_paths::test_env_lock()
            .lock()
            .expect("env lock");
        let temp_root =
            env::temp_dir().join(format!("mihomo-switch-prepared-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).expect("create temp runtime");
        env::set_var("MIHOMO_MANAGER_HOME", &temp_root);

        let old_node = sample_node("old node", "old.example.com", 443);
        let new_node = sample_node("new node", "new.example.com", 8443);
        let canonical = write_pool_config(&settings_with_slots(
            vec![old_node.clone()],
            vec![slot("old node", 10801, &old_node, true)],
        ))
        .expect("write old config")
        .expect("canonical config");
        let old_text = fs::read_to_string(&canonical).expect("read old config");

        let prepared = prepare_pool_config(&settings_with_slots(
            vec![new_node.clone()],
            vec![slot("new node", 10801, &new_node, true)],
        ))
        .expect("prepare new config");
        assert_eq!(
            fs::read_to_string(&canonical).expect("canonical before commit"),
            old_text
        );
        drop(prepared);
        assert_eq!(
            fs::read_to_string(&canonical).expect("canonical after discard"),
            old_text
        );

        let prepared = prepare_pool_config(&settings_with_slots(
            vec![new_node.clone()],
            vec![slot("new node", 10801, &new_node, true)],
        ))
        .expect("prepare new config");
        prepared.commit().expect("commit new config");
        let committed = fs::read_to_string(&canonical).expect("read committed config");
        assert!(committed.contains("new.example.com"));
        assert!(!committed.contains("old.example.com"));

        env::remove_var("MIHOMO_MANAGER_HOME");
        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn collect_active_slots_skips_disabled_and_unbound() {
        let node_a = sample_node("node", "one.example.com", 443);
        let mut disabled = slot("node", 10802, &node_a, false);
        disabled.id = "disabled".to_string();
        let settings = settings_with_slots(
            vec![node_a.clone()],
            vec![slot("node", 10801, &node_a, true), disabled],
        );

        let active = collect_active_slots(&settings);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].local_port, 10801);
        assert_eq!(active[0].node.name, "node");
    }

    #[test]
    fn write_pool_config_rejects_duplicate_listener_ports() {
        let _guard = crate::runtime_paths::test_env_lock()
            .lock()
            .expect("env lock");
        let temp_root = env::temp_dir().join(format!(
            "mihomo-switch-duplicate-port-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).expect("create temp runtime");
        env::set_var("MIHOMO_MANAGER_HOME", &temp_root);

        let node_a = sample_node("node-a", "one.example.com", 443);
        let node_b = sample_node("node-b", "two.example.com", 443);
        let settings = settings_with_slots(
            vec![node_a.clone(), node_b.clone()],
            vec![
                slot("slot-a", 10801, &node_a, true),
                slot("slot-b", 10801, &node_b, true),
            ],
        );

        let error = write_pool_config(&settings).expect_err("duplicate ports must be rejected");
        assert!(error.to_string().contains("重复使用"));

        env::remove_var("MIHOMO_MANAGER_HOME");
        let _ = fs::remove_dir_all(&temp_root);
    }
}
