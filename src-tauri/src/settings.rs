use std::{collections::BTreeMap, fs, net::TcpListener, path::Path};

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::{
    models::{
        AppSettings, MigrationReport, NodeBinding, PortSlot, PortSlotBindingInput,
        PortSlotBindingView, PortSlotInput, PortSlotView, PortValidation, ProxyNode,
        SubscriptionRecord, UpsertSubscriptionInput,
    },
    runtime_paths,
};

fn write_json_atomic(path: &Path, settings: &AppSettings) -> Result<()> {
    let temp_path = path.with_file_name(format!("settings.{}.json.tmp", Uuid::new_v4()));
    let backup_path = path.with_file_name(format!("settings.{}.json.bak", Uuid::new_v4()));
    let text = serde_json::to_string_pretty(settings)?;
    fs::write(&temp_path, text)
        .with_context(|| format!("write temp settings: {}", temp_path.display()))?;
    let had_existing = path.exists();
    if had_existing {
        fs::rename(path, &backup_path)
            .with_context(|| format!("stage existing settings: {}", path.display()))?;
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        if had_existing {
            let _ = fs::rename(&backup_path, path);
        }
        return Err(error).with_context(|| format!("replace settings: {}", path.display()));
    }
    if had_existing {
        let _ = fs::remove_file(backup_path);
    }
    Ok(())
}

fn backup_corrupt_settings(path: &Path) -> Result<()> {
    let backup_path = path.with_file_name(format!("settings.corrupt-{}.json", Uuid::new_v4()));
    fs::copy(path, &backup_path).with_context(|| {
        format!(
            "backup corrupt settings: {} -> {}",
            path.display(),
            backup_path.display()
        )
    })?;
    Ok(())
}

fn migrate_sub(sub: &mut SubscriptionRecord) -> bool {
    let mut changed = false;
    if sub.id.is_empty() {
        sub.id = Uuid::new_v4().to_string();
        changed = true;
    }
    if sub.ua.is_empty() {
        sub.ua = crate::models::default_ua();
        changed = true;
    }
    if sub.start_port == 0 {
        sub.start_port = 10801;
        changed = true;
    }
    sanitize_subscription(sub) || changed
}

fn parse_index_key(key: &str) -> Option<usize> {
    key.parse::<usize>().ok()
}

fn can_bind_local_port(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

const NODE_FINGERPRINT_VERSION: &str = "v2:";

fn legacy_node_fingerprint(node: &ProxyNode) -> String {
    format!("{}|{}|{}", node.node_type, node.server, node.port)
}

/// Full identity of a node, stable across subscription reordering while still
/// distinguishing display names, credentials, and transport options.
///
/// Including the display name is deliberate: when an upstream update replaces
/// a node with a similar endpoint, the backend must not silently guess that it
/// is the user's previous binding.
pub fn node_fingerprint(node: &ProxyNode) -> String {
    format!(
        "{NODE_FINGERPRINT_VERSION}{}",
        serde_json::to_string(node).unwrap_or_default()
    )
}

/// Resolve a binding against the current node list of its subscription.
///
/// Matches primarily by fingerprint; when several nodes share a fingerprint the
/// one whose name matches the cached binding name wins, so duplicates stay
/// stable. Returns the live index and node, or None when the node is gone.
pub fn resolve_binding<'a>(
    sub: &'a SubscriptionRecord,
    binding: &NodeBinding,
) -> Option<(usize, &'a ProxyNode)> {
    if !binding.fingerprint.starts_with(NODE_FINGERPRINT_VERSION) {
        let mut matches = sub.nodes.iter().enumerate().filter(|(_, node)| {
            legacy_node_fingerprint(node) == binding.fingerprint && node.name == binding.node_name
        });
        let first = matches.next();
        return if matches.next().is_none() {
            first
        } else {
            None
        };
    }

    for (index, node) in sub.nodes.iter().enumerate() {
        if node_fingerprint(node) != binding.fingerprint {
            continue;
        }
        return Some((index, node));
    }
    None
}

/// Build the render-ready slot views by resolving every binding against the
/// live subscriptions. Never mutates persisted state.
pub fn build_slot_views(settings: &AppSettings) -> Vec<PortSlotView> {
    settings
        .port_slots
        .iter()
        .map(|slot| {
            let Some(binding) = &slot.binding else {
                return PortSlotView {
                    id: slot.id.clone(),
                    name: slot.name.clone(),
                    note: slot.note.clone(),
                    local_port: slot.local_port,
                    enabled: slot.enabled,
                    state: "unbound".to_string(),
                    invalid_reason: None,
                    binding: None,
                };
            };

            let sub = find_subscription(settings, &binding.sub_id);
            let Some(sub) = sub else {
                return PortSlotView {
                    id: slot.id.clone(),
                    name: slot.name.clone(),
                    note: slot.note.clone(),
                    local_port: slot.local_port,
                    enabled: slot.enabled,
                    state: "invalid".to_string(),
                    invalid_reason: Some("所属订阅已删除".to_string()),
                    binding: Some(PortSlotBindingView {
                        sub_id: binding.sub_id.clone(),
                        sub_name: "未知订阅".to_string(),
                        node_index: None,
                        node_name: binding.node_name.clone(),
                        node_type: String::new(),
                        server: String::new(),
                        server_port: 0,
                    }),
                };
            };

            match resolve_binding(sub, binding) {
                Some((index, node)) => PortSlotView {
                    id: slot.id.clone(),
                    name: slot.name.clone(),
                    note: slot.note.clone(),
                    local_port: slot.local_port,
                    enabled: slot.enabled,
                    state: "valid".to_string(),
                    invalid_reason: None,
                    binding: Some(PortSlotBindingView {
                        sub_id: sub.id.clone(),
                        sub_name: sub.name.clone(),
                        node_index: Some(index),
                        node_name: node.name.clone(),
                        node_type: node.node_type.clone(),
                        server: node.server.clone(),
                        server_port: node.port,
                    }),
                },
                None => PortSlotView {
                    id: slot.id.clone(),
                    name: slot.name.clone(),
                    note: slot.note.clone(),
                    local_port: slot.local_port,
                    enabled: slot.enabled,
                    state: "invalid".to_string(),
                    invalid_reason: Some("节点已失效".to_string()),
                    binding: Some(PortSlotBindingView {
                        sub_id: sub.id.clone(),
                        sub_name: sub.name.clone(),
                        node_index: None,
                        node_name: binding.node_name.clone(),
                        node_type: String::new(),
                        server: String::new(),
                        server_port: 0,
                    }),
                },
            }
        })
        .collect()
}

fn sanitize_port_slots(settings: &mut AppSettings) -> bool {
    let original = serde_json::to_string(&settings.port_slots).unwrap_or_default();
    let mut seen_ids = std::collections::BTreeSet::new();
    settings.port_slots.retain_mut(|slot| {
        if slot.id.trim().is_empty() {
            slot.id = Uuid::new_v4().to_string();
        }
        if !seen_ids.insert(slot.id.clone()) {
            return false;
        }
        if slot.local_port == 0 {
            return false;
        }
        if let Some(binding) = &slot.binding {
            if binding.sub_id.trim().is_empty() || binding.fingerprint.trim().is_empty() {
                slot.binding = None;
            }
        }
        true
    });
    serde_json::to_string(&settings.port_slots).unwrap_or_default() != original
}

fn upgrade_legacy_bindings(settings: &mut AppSettings) -> bool {
    let subscriptions = &settings.subscriptions;
    let mut changed = false;

    for slot in &mut settings.port_slots {
        let Some(binding) = slot.binding.as_mut() else {
            continue;
        };
        if binding.fingerprint.starts_with(NODE_FINGERPRINT_VERSION) {
            continue;
        }
        let Some(sub) = subscriptions.iter().find(|sub| sub.id == binding.sub_id) else {
            continue;
        };
        let Some((_, node)) = resolve_binding(sub, binding) else {
            continue;
        };
        binding.fingerprint = node_fingerprint(node);
        binding.node_name = node.name.clone();
        changed = true;
    }

    changed
}

/// One-time migration of the legacy node-driven selection into port slots.
///
/// Non-destructive: the legacy `selected_node_indices` / `port_assignments` are
/// left intact as a backup. Returns a report only when a migration actually ran.
pub fn ensure_port_slots(settings: &mut AppSettings) -> Option<MigrationReport> {
    if settings.slots_migrated {
        return None;
    }

    let mut report = MigrationReport {
        migrated: true,
        created_slots: 0,
        messages: Vec::new(),
    };
    let mut used_ports = settings
        .port_slots
        .iter()
        .map(|slot| slot.local_port)
        .collect::<std::collections::BTreeSet<u16>>();

    let subscriptions = settings.subscriptions.clone();
    for sub in &subscriptions {
        for index in &sub.selected_node_indices {
            let Some(node) = sub.nodes.get(*index) else {
                continue;
            };
            let Some(port) = sub.port_assignments.get(&index.to_string()).copied() else {
                report
                    .messages
                    .push(format!("跳过“{}”: 缺少端口分配", node.name));
                continue;
            };
            if used_ports.contains(&port) {
                report
                    .messages
                    .push(format!("跳过“{}”: 端口 {} 已被占用", node.name, port));
                continue;
            }

            let remark = sub.node_remarks.get(&index.to_string()).cloned();
            let name = remark
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| {
                    if node.name.trim().is_empty() {
                        format!("端口 {port}")
                    } else {
                        node.name.clone()
                    }
                });
            settings.port_slots.push(PortSlot {
                id: Uuid::new_v4().to_string(),
                name,
                note: String::new(),
                local_port: port,
                enabled: true,
                binding: Some(NodeBinding {
                    sub_id: sub.id.clone(),
                    fingerprint: node_fingerprint(node),
                    node_name: node.name.clone(),
                }),
            });
            used_ports.insert(port);
            report.created_slots += 1;
        }
    }

    settings.slots_migrated = true;
    if report.created_slots > 0 {
        report
            .messages
            .insert(0, format!("已从旧配置迁移 {} 个端口", report.created_slots));
    }
    Some(report)
}

/// Validate a candidate local port against range, sibling slots, and the OS.
pub fn validate_local_port(
    settings: &AppSettings,
    port: u16,
    ignore_slot_id: Option<&str>,
) -> PortValidation {
    if port < 1024 {
        return PortValidation {
            status: "invalid".to_string(),
            message: "端口需在 1024 - 65535 之间".to_string(),
        };
    }
    let conflict = settings
        .port_slots
        .iter()
        .any(|slot| slot.local_port == port && Some(slot.id.as_str()) != ignore_slot_id);
    if conflict {
        return PortValidation {
            status: "conflict".to_string(),
            message: format!("端口 {port} 已被其它端口槽位使用"),
        };
    }
    let unchanged_slot_port = ignore_slot_id
        .and_then(|id| settings.port_slots.iter().find(|slot| slot.id == id))
        .map(|slot| slot.local_port == port)
        .unwrap_or(false);
    if unchanged_slot_port {
        return PortValidation {
            status: "ok".to_string(),
            message: "端口可用".to_string(),
        };
    }
    if !can_bind_local_port(port) {
        return PortValidation {
            status: "occupied".to_string(),
            message: format!("端口 {port} 已被系统其它程序占用"),
        };
    }
    PortValidation {
        status: "ok".to_string(),
        message: "端口可用".to_string(),
    }
}

fn build_binding(
    settings: &AppSettings,
    input: &PortSlotBindingInput,
) -> Result<NodeBinding, String> {
    let sub = find_subscription(settings, &input.sub_id).ok_or("目标订阅不存在")?;
    let node = sub.nodes.get(input.node_index).ok_or("目标节点不存在")?;
    Ok(NodeBinding {
        sub_id: sub.id.clone(),
        fingerprint: node_fingerprint(node),
        node_name: node.name.clone(),
    })
}

pub fn create_port_slot(
    settings: &mut AppSettings,
    input: PortSlotInput,
) -> Result<PortSlot, String> {
    let validation = validate_local_port(settings, input.local_port, None);
    if validation.status != "ok" {
        return Err(validation.message);
    }
    let binding = match &input.binding {
        Some(value) => Some(build_binding(settings, value)?),
        None => None,
    };
    let name = if input.name.trim().is_empty() {
        format!("端口 {}", input.local_port)
    } else {
        input.name.trim().to_string()
    };
    let slot = PortSlot {
        id: Uuid::new_v4().to_string(),
        name,
        note: input.note.trim().to_string(),
        local_port: input.local_port,
        enabled: input.enabled,
        binding,
    };
    settings.port_slots.push(slot.clone());
    Ok(slot)
}

pub fn update_port_slot(
    settings: &mut AppSettings,
    slot_id: &str,
    input: PortSlotInput,
) -> Result<(), String> {
    if !settings.port_slots.iter().any(|slot| slot.id == slot_id) {
        return Err("目标端口不存在".to_string());
    }
    let validation = validate_local_port(settings, input.local_port, Some(slot_id));
    if validation.status != "ok" {
        return Err(validation.message);
    }
    let binding = match &input.binding {
        Some(value) => Some(build_binding(settings, value)?),
        None => None,
    };
    let name = if input.name.trim().is_empty() {
        format!("端口 {}", input.local_port)
    } else {
        input.name.trim().to_string()
    };
    if let Some(slot) = settings
        .port_slots
        .iter_mut()
        .find(|slot| slot.id == slot_id)
    {
        slot.name = name;
        slot.note = input.note.trim().to_string();
        slot.local_port = input.local_port;
        slot.enabled = input.enabled;
        slot.binding = binding;
    }
    Ok(())
}

pub fn delete_port_slot(settings: &mut AppSettings, slot_id: &str) -> bool {
    let before = settings.port_slots.len();
    settings.port_slots.retain(|slot| slot.id != slot_id);
    settings.port_slots.len() != before
}

pub fn set_slot_enabled(settings: &mut AppSettings, slot_id: &str, enabled: bool) -> bool {
    if let Some(slot) = settings
        .port_slots
        .iter_mut()
        .find(|slot| slot.id == slot_id)
    {
        slot.enabled = enabled;
        true
    } else {
        false
    }
}

pub fn bind_slot_node(
    settings: &mut AppSettings,
    slot_id: &str,
    input: PortSlotBindingInput,
) -> Result<(), String> {
    let binding = build_binding(settings, &input)?;
    let slot = settings
        .port_slots
        .iter_mut()
        .find(|slot| slot.id == slot_id)
        .ok_or("目标端口不存在")?;
    slot.binding = Some(binding);
    Ok(())
}

pub fn clear_slot_binding(settings: &mut AppSettings, slot_id: &str) -> bool {
    if let Some(slot) = settings
        .port_slots
        .iter_mut()
        .find(|slot| slot.id == slot_id)
    {
        slot.binding = None;
        true
    } else {
        false
    }
}

pub fn reorder_port_slots(settings: &mut AppSettings, ordered_ids: &[String]) {
    if ordered_ids.len() != settings.port_slots.len() {
        return;
    }
    let existing = settings
        .port_slots
        .iter()
        .map(|slot| slot.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let next = ordered_ids
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if existing != next {
        return;
    }
    let mut items = std::mem::take(&mut settings.port_slots)
        .into_iter()
        .map(|slot| (slot.id.clone(), slot))
        .collect::<BTreeMap<_, _>>();
    settings.port_slots = ordered_ids
        .iter()
        .filter_map(|id| items.remove(id))
        .collect();
}

fn sanitize_subscription(sub: &mut SubscriptionRecord) -> bool {
    let original_selected = sub.selected_node_indices.clone();
    let original_ports = sub.port_assignments.clone();
    let original_remarks = sub.node_remarks.clone();

    sub.selected_node_indices.sort_unstable();
    sub.selected_node_indices.dedup();
    sub.selected_node_indices
        .retain(|index| *index < sub.nodes.len());

    let selected = sub
        .selected_node_indices
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    sub.port_assignments.retain(|key, _| {
        parse_index_key(key)
            .map(|index| selected.contains(&index))
            .unwrap_or(false)
    });
    sub.node_remarks.retain(|key, value| {
        parse_index_key(key)
            .map(|index| index < sub.nodes.len() && !value.trim().is_empty())
            .unwrap_or(false)
    });

    sub.selected_node_indices != original_selected
        || sub.port_assignments != original_ports
        || sub.node_remarks != original_remarks
}

fn normalize_settings(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    if settings.schema_version != 4 {
        settings.schema_version = 4;
        changed = true;
    }

    for sub in &mut settings.subscriptions {
        changed |= migrate_sub(sub);
    }

    changed |= sanitize_port_slots(settings);
    changed |= upgrade_legacy_bindings(settings);

    if settings.local_proxy_url.trim().is_empty() {
        settings.local_proxy_url = crate::models::default_local_proxy_url();
        changed = true;
    }
    if settings.mihomo_path.trim().is_empty() {
        settings.mihomo_path = crate::models::default_mihomo_path_string();
        changed = true;
    }

    changed
}

pub fn load_settings() -> Result<AppSettings> {
    let path = runtime_paths::settings_path()?;
    if !path.exists() {
        let defaults = AppSettings::default();
        save_settings(&defaults)?;
        return Ok(defaults);
    }

    let content =
        fs::read_to_string(&path).with_context(|| format!("read settings: {}", path.display()))?;
    let mut needs_save = false;
    let mut settings: AppSettings = match serde_json::from_str(&content) {
        Ok(settings) => settings,
        Err(_) => {
            backup_corrupt_settings(&path)?;
            needs_save = true;
            AppSettings::default()
        }
    };
    needs_save |= normalize_settings(&mut settings);
    if needs_save {
        save_settings(&settings)?;
    }
    Ok(settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = runtime_paths::settings_path()?;
    write_json_atomic(&path, settings)
}

pub fn calc_start_port(settings: &AppSettings) -> u16 {
    let max_start = settings
        .subscriptions
        .iter()
        .map(|item| item.start_port.max(10801))
        .max()
        .unwrap_or(10801);
    (((max_start - 1) / 100) + 1) * 100 + 1
}

pub fn create_subscription(
    settings: &mut AppSettings,
    input: UpsertSubscriptionInput,
) -> Result<SubscriptionRecord> {
    let record = SubscriptionRecord {
        id: Uuid::new_v4().to_string(),
        name: if input.name.trim().is_empty() {
            format!("订阅{}", settings.subscriptions.len() + 1)
        } else {
            input.name.trim().to_string()
        },
        url: input.url.trim().to_string(),
        ua: crate::models::default_ua(),
        start_port: calc_start_port(settings),
        manual: input.manual,
        content: input.content,
        nodes: Vec::<ProxyNode>::new(),
        selected_node_indices: Vec::new(),
        port_assignments: BTreeMap::new(),
        node_remarks: BTreeMap::new(),
    };
    settings.subscriptions.push(record.clone());
    Ok(record)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateSubscriptionOutcome {
    pub source_changed: bool,
    pub cleared_nodes: bool,
}

pub fn update_subscription(
    settings: &mut AppSettings,
    sub_id: &str,
    input: UpsertSubscriptionInput,
) -> Result<UpdateSubscriptionOutcome> {
    let mut outcome = UpdateSubscriptionOutcome::default();
    if let Some(sub) = settings
        .subscriptions
        .iter_mut()
        .find(|item| item.id == sub_id)
    {
        let next_name = input.name.trim();
        if !next_name.is_empty() {
            sub.name = next_name.to_string();
        }
        let was_manual = sub.manual;
        sub.manual = input.manual;
        if input.manual {
            outcome.source_changed = !was_manual || sub.content != input.content;
            sub.content = input.content;
            sub.nodes.clear();
            sub.selected_node_indices.clear();
            sub.port_assignments.clear();
            sub.node_remarks.clear();
            outcome.cleared_nodes = true;
        } else {
            let next_url = input.url.trim().to_string();
            outcome.source_changed = was_manual || sub.url != next_url;
            sub.url = next_url;
            sub.content.clear();
            if outcome.source_changed {
                sub.nodes.clear();
                sub.selected_node_indices.clear();
                sub.port_assignments.clear();
                sub.node_remarks.clear();
                outcome.cleared_nodes = true;
            }
        }
        let _ = sanitize_subscription(sub);
    }
    Ok(outcome)
}

pub fn delete_subscription(settings: &mut AppSettings, sub_id: &str) {
    settings.subscriptions.retain(|item| item.id != sub_id);
}

pub fn delete_selected_nodes(
    settings: &mut AppSettings,
    sub_id: &str,
    node_indices: &[usize],
) -> usize {
    let Some(index) = settings
        .subscriptions
        .iter()
        .position(|item| item.id == sub_id)
    else {
        return 0;
    };

    let removed = node_indices
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if removed.is_empty() {
        return 0;
    }

    let sub = &mut settings.subscriptions[index];
    let old_nodes = std::mem::take(&mut sub.nodes);
    let old_ports = std::mem::take(&mut sub.port_assignments);
    let old_remarks = std::mem::take(&mut sub.node_remarks);

    let mut next_nodes = Vec::with_capacity(old_nodes.len().saturating_sub(removed.len()));
    let mut next_ports = BTreeMap::new();
    let mut next_remarks = BTreeMap::new();
    let mut removed_count = 0;

    for (old_index, node) in old_nodes.into_iter().enumerate() {
        if removed.contains(&old_index) {
            removed_count += 1;
            continue;
        }

        let new_index = next_nodes.len();
        next_nodes.push(node);
        if let Some(port) = old_ports.get(&old_index.to_string()) {
            next_ports.insert(new_index.to_string(), *port);
        }
        if let Some(remark) = old_remarks.get(&old_index.to_string()) {
            next_remarks.insert(new_index.to_string(), remark.clone());
        }
    }

    sub.nodes = next_nodes;
    sub.selected_node_indices.clear();
    sub.port_assignments = next_ports;
    sub.node_remarks = next_remarks;
    let _ = sanitize_subscription(sub);
    removed_count
}

pub fn save_node_remark(
    settings: &mut AppSettings,
    sub_id: &str,
    node_index: usize,
    remark: String,
) -> bool {
    let Some(sub) = settings
        .subscriptions
        .iter_mut()
        .find(|item| item.id == sub_id)
    else {
        return false;
    };
    if node_index >= sub.nodes.len() {
        return false;
    }

    let key = node_index.to_string();
    let trimmed = remark.trim().to_string();
    if trimmed.is_empty() {
        sub.node_remarks.remove(&key);
    } else {
        sub.node_remarks.insert(key, trimmed);
    }
    let _ = sanitize_subscription(sub);
    true
}

pub fn reorder_subscriptions(settings: &mut AppSettings, ordered_ids: &[String]) {
    if ordered_ids.len() != settings.subscriptions.len() {
        return;
    }

    let existing_ids = settings
        .subscriptions
        .iter()
        .map(|item| item.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let next_ids = ordered_ids
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();

    if existing_ids != next_ids {
        return;
    }

    let mut items = std::mem::take(&mut settings.subscriptions)
        .into_iter()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();

    settings.subscriptions = ordered_ids
        .iter()
        .filter_map(|id| items.remove(id))
        .collect();
}

pub fn find_subscription_mut<'a>(
    settings: &'a mut AppSettings,
    sub_id: &str,
) -> Option<&'a mut SubscriptionRecord> {
    settings
        .subscriptions
        .iter_mut()
        .find(|item| item.id == sub_id)
}

pub fn find_subscription<'a>(
    settings: &'a AppSettings,
    sub_id: &str,
) -> Option<&'a SubscriptionRecord> {
    settings.subscriptions.iter().find(|item| item.id == sub_id)
}

pub fn assign_ports(
    settings: &AppSettings,
    current_sub_id: &str,
    selected: &[usize],
    ports: &mut BTreeMap<String, u16>,
) {
    let current_start = settings
        .subscriptions
        .iter()
        .find(|sub| sub.id == current_sub_id)
        .map(|sub| sub.start_port)
        .unwrap_or(10801);

    let mut used_ports = settings
        .subscriptions
        .iter()
        .filter(|sub| sub.id != current_sub_id)
        .flat_map(|sub| sub.port_assignments.values().copied())
        .collect::<std::collections::BTreeSet<u16>>();

    used_ports.extend(ports.values().copied());

    let mut next_port = current_start;
    for value in ports.values() {
        if *value >= next_port {
            next_port = value.saturating_add(1);
        }
    }

    for &index in selected {
        let key = index.to_string();
        if ports.contains_key(&key) {
            continue;
        }
        while used_ports.contains(&next_port) || !can_bind_local_port(next_port) {
            let Some(candidate) = next_port.checked_add(1) else {
                return;
            };
            next_port = candidate;
        }
        ports.insert(key, next_port);
        used_ports.insert(next_port);
        next_port = next_port.saturating_add(1);
    }
}

fn normalize_existing_ports(
    settings: &AppSettings,
    current_sub_id: &str,
    selected: &[usize],
    ports: &mut BTreeMap<String, u16>,
    previous_ports: Option<&BTreeMap<String, u16>>,
    allow_unbindable_existing: bool,
) {
    let selected_set = selected
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    ports.retain(|key, port| {
        parse_index_key(key)
            .map(|index| selected_set.contains(&index) && *port >= 1024)
            .unwrap_or(false)
    });

    let mut used_ports = settings
        .subscriptions
        .iter()
        .filter(|sub| sub.id != current_sub_id)
        .flat_map(|sub| sub.port_assignments.values().copied())
        .collect::<std::collections::BTreeSet<u16>>();
    let mut current_seen = std::collections::BTreeSet::new();

    for index in selected {
        let key = index.to_string();
        let Some(port) = ports.get(&key).copied() else {
            continue;
        };
        let port_unchanged =
            previous_ports.and_then(|items| items.get(&key)).copied() == Some(port);
        let port_available =
            can_bind_local_port(port) || (allow_unbindable_existing && port_unchanged);
        if used_ports.contains(&port) || current_seen.contains(&port) || !port_available {
            ports.remove(&key);
            continue;
        }
        current_seen.insert(port);
        used_ports.insert(port);
    }
}

pub fn recalculate_ports(settings: &mut AppSettings, current_sub_id: &str) {
    recalculate_ports_with_previous(settings, current_sub_id, None, false);
}

pub fn recalculate_ports_with_previous(
    settings: &mut AppSettings,
    current_sub_id: &str,
    previous_ports: Option<&BTreeMap<String, u16>>,
    allow_unbindable_existing: bool,
) {
    let Some(index) = settings
        .subscriptions
        .iter()
        .position(|item| item.id == current_sub_id)
    else {
        return;
    };

    let _ = sanitize_subscription(&mut settings.subscriptions[index]);
    let selected = settings.subscriptions[index].selected_node_indices.clone();
    let mut ports = settings.subscriptions[index].port_assignments.clone();
    normalize_existing_ports(
        settings,
        current_sub_id,
        &selected,
        &mut ports,
        previous_ports,
        allow_unbindable_existing,
    );
    assign_ports(settings, current_sub_id, &selected, &mut ports);
    settings.subscriptions[index].port_assignments = ports;
}

#[cfg(test)]
mod tests {
    use std::{env, ffi::OsString, net::TcpListener, path::PathBuf};

    use super::*;

    struct TestEnv {
        _guard: std::sync::MutexGuard<'static, ()>,
        previous_local_appdata: Option<OsString>,
        root: PathBuf,
    }

    impl TestEnv {
        fn new(name: &str) -> Self {
            let guard = runtime_paths::test_env_lock()
                .lock()
                .expect("test env lock");
            let root = std::env::temp_dir()
                .join("mihomo-switch-settings-tests")
                .join(name);
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("create temp runtime dir");
            let local_appdata = root.join("local-appdata");
            std::fs::create_dir_all(&local_appdata).expect("create temp local appdata dir");
            let previous_local_appdata = env::var_os("LOCALAPPDATA");
            env::set_var("MIHOMO_MANAGER_HOME", &root);
            env::set_var("LOCALAPPDATA", &local_appdata);
            env::remove_var("MIHOMO_MANAGER_SKIP_LEGACY_IMPORT");
            Self {
                _guard: guard,
                previous_local_appdata,
                root,
            }
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            env::remove_var("MIHOMO_MANAGER_HOME");
            env::remove_var("MIHOMO_MANAGER_SKIP_LEGACY_IMPORT");
            if let Some(value) = &self.previous_local_appdata {
                env::set_var("LOCALAPPDATA", value);
            } else {
                env::remove_var("LOCALAPPDATA");
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn build_sub(
        id: &str,
        start_port: u16,
        node_count: usize,
        selected: Vec<usize>,
        ports: &[(&str, u16)],
    ) -> SubscriptionRecord {
        SubscriptionRecord {
            id: id.to_string(),
            name: id.to_string(),
            url: String::new(),
            ua: "clash.meta".to_string(),
            start_port,
            manual: false,
            content: String::new(),
            nodes: (0..node_count)
                .map(|index| ProxyNode {
                    name: format!("node-{index}"),
                    node_type: "vmess".to_string(),
                    server: format!("server-{index}"),
                    port: 443,
                    ..ProxyNode::default()
                })
                .collect(),
            selected_node_indices: selected,
            port_assignments: ports
                .iter()
                .map(|(key, value)| ((*key).to_string(), *value))
                .collect(),
            node_remarks: BTreeMap::new(),
        }
    }

    #[test]
    fn recalculate_ports_removes_stale_indices_and_ports() {
        let mut settings = AppSettings {
            schema_version: 2,
            subscriptions: vec![build_sub(
                "sub-a",
                10801,
                2,
                vec![1, 1, 3],
                &[("0", 10801), ("1", 10802), ("3", 10803)],
            )],
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        };

        recalculate_ports(&mut settings, "sub-a");

        let sub = &settings.subscriptions[0];
        assert_eq!(sub.selected_node_indices, vec![1]);
        assert_eq!(sub.port_assignments.len(), 1);
        assert_eq!(sub.port_assignments.get("1"), Some(&10802));
    }

    #[test]
    fn recalculate_ports_reassigns_conflicting_ports() {
        let mut settings = AppSettings {
            schema_version: 2,
            subscriptions: vec![
                build_sub("sub-a", 10801, 1, vec![0], &[("0", 10801)]),
                build_sub("sub-b", 10801, 1, vec![0], &[("0", 10801)]),
            ],
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        };

        recalculate_ports(&mut settings, "sub-b");

        let sub = &settings.subscriptions[1];
        assert_ne!(sub.port_assignments.get("0"), Some(&10801));
    }

    #[test]
    fn recalculate_ports_reassigns_duplicate_ports_in_same_subscription() {
        let mut settings = AppSettings {
            schema_version: 2,
            subscriptions: vec![build_sub(
                "sub-a",
                10801,
                2,
                vec![0, 1],
                &[("0", 10801), ("1", 10801)],
            )],
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        };

        recalculate_ports(&mut settings, "sub-a");

        let sub = &settings.subscriptions[0];
        assert_ne!(sub.port_assignments.get("0"), sub.port_assignments.get("1"));
    }

    #[test]
    fn recalculate_ports_skips_system_occupied_ports_for_new_assignments() {
        let occupied = TcpListener::bind("127.0.0.1:0").expect("bind occupied port");
        let occupied_port = occupied.local_addr().expect("occupied address").port();
        let mut settings = AppSettings {
            schema_version: 2,
            subscriptions: vec![build_sub("sub-a", occupied_port, 1, vec![0], &[])],
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        };

        recalculate_ports(&mut settings, "sub-a");

        let assigned = settings.subscriptions[0]
            .port_assignments
            .get("0")
            .copied()
            .expect("assigned port");
        assert_ne!(assigned, occupied_port);
    }

    #[test]
    fn recalculate_ports_reassigns_newly_changed_occupied_port() {
        let occupied = TcpListener::bind("127.0.0.1:0").expect("bind occupied port");
        let occupied_port = occupied.local_addr().expect("occupied address").port();
        let previous_ports = [("0".to_string(), 10801)]
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let mut settings = AppSettings {
            schema_version: 2,
            subscriptions: vec![build_sub(
                "sub-a",
                10801,
                1,
                vec![0],
                &[("0", occupied_port)],
            )],
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        };

        recalculate_ports_with_previous(&mut settings, "sub-a", Some(&previous_ports), true);

        let assigned = settings.subscriptions[0]
            .port_assignments
            .get("0")
            .copied()
            .expect("assigned port");
        assert_ne!(assigned, occupied_port);
    }

    #[test]
    fn delete_selected_nodes_reindexes_remaining_ports() {
        let mut settings = AppSettings {
            schema_version: 2,
            subscriptions: vec![build_sub(
                "sub-a",
                10801,
                4,
                vec![0, 2],
                &[("0", 10801), ("1", 10802), ("2", 10803), ("3", 10804)],
            )],
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        };

        let removed = delete_selected_nodes(&mut settings, "sub-a", &[0, 2]);
        let sub = &settings.subscriptions[0];

        assert_eq!(removed, 2);
        assert_eq!(sub.nodes.len(), 2);
        assert!(sub.selected_node_indices.is_empty());
        assert!(sub.port_assignments.is_empty());
    }

    #[test]
    fn node_fingerprint_distinguishes_credentials_and_names_on_same_endpoint() {
        let first = ProxyNode {
            name: "shared endpoint".to_string(),
            node_type: "trojan".to_string(),
            server: "same.example.com".to_string(),
            port: 443,
            uuid: "password-a".to_string(),
            ..ProxyNode::default()
        };
        let mut second = first.clone();
        second.uuid = "password-b".to_string();

        assert_ne!(node_fingerprint(&first), node_fingerprint(&second));

        let mut renamed = first.clone();
        renamed.name = "renamed".to_string();
        assert_ne!(node_fingerprint(&first), node_fingerprint(&renamed));
    }

    #[test]
    fn resolve_binding_survives_reorder_but_not_display_name_change() {
        let original = ProxyNode {
            name: "old name".to_string(),
            node_type: "vless".to_string(),
            server: "node.example.com".to_string(),
            port: 443,
            uuid: "node-id".to_string(),
            ..ProxyNode::default()
        };
        let renamed = ProxyNode {
            name: "new name".to_string(),
            ..original.clone()
        };
        let sub = build_sub("sub-a", 10801, 0, Vec::new(), &[]);
        let sub = SubscriptionRecord {
            nodes: vec![
                ProxyNode {
                    name: "other".to_string(),
                    server: "other.example.com".to_string(),
                    ..ProxyNode::default()
                },
                original.clone(),
                renamed,
            ],
            ..sub
        };
        let binding = NodeBinding {
            sub_id: "sub-a".to_string(),
            fingerprint: node_fingerprint(&original),
            node_name: original.name,
        };

        let (index, node) =
            resolve_binding(&sub, &binding).expect("binding should survive reorder");
        assert_eq!(index, 1);
        assert_eq!(node.name, "old name");

        let renamed_only = SubscriptionRecord {
            nodes: vec![sub.nodes[2].clone()],
            ..sub
        };
        assert!(resolve_binding(&renamed_only, &binding).is_none());
    }

    #[test]
    fn legacy_binding_does_not_guess_between_duplicate_nodes() {
        let node = ProxyNode {
            name: "duplicate".to_string(),
            node_type: "trojan".to_string(),
            server: "same.example.com".to_string(),
            port: 443,
            uuid: "password-a".to_string(),
            ..ProxyNode::default()
        };
        let mut other = node.clone();
        other.uuid = "password-b".to_string();
        let sub = SubscriptionRecord {
            nodes: vec![node.clone(), other],
            ..build_sub("sub-a", 10801, 0, Vec::new(), &[])
        };
        let binding = NodeBinding {
            sub_id: "sub-a".to_string(),
            fingerprint: legacy_node_fingerprint(&node),
            node_name: node.name,
        };

        assert!(resolve_binding(&sub, &binding).is_none());
    }

    #[test]
    fn validate_local_port_allows_unchanged_port_when_it_is_currently_bound() {
        let occupied = TcpListener::bind("127.0.0.1:0").expect("bind occupied port");
        let occupied_port = occupied.local_addr().expect("occupied address").port();
        let settings = AppSettings {
            port_slots: vec![PortSlot {
                id: "slot-a".to_string(),
                name: "slot-a".to_string(),
                note: String::new(),
                local_port: occupied_port,
                enabled: true,
                binding: None,
            }],
            slots_migrated: true,
            ..AppSettings::default()
        };

        let validation = validate_local_port(&settings, occupied_port, Some("slot-a"));
        assert_eq!(validation.status, "ok");
    }

    #[test]
    fn load_settings_does_not_import_legacy_user_settings() {
        let _env = TestEnv::new("no-legacy-import");

        let mut legacy = AppSettings::default();
        legacy.subscriptions = vec![build_sub("legacy-sub", 10801, 1, vec![0], &[("0", 10801)])];

        let legacy_path =
            runtime_paths::legacy_runtime_settings_path().expect("legacy settings path");
        std::fs::create_dir_all(legacy_path.parent().expect("legacy settings parent"))
            .expect("create legacy settings parent");
        std::fs::write(
            &legacy_path,
            serde_json::to_string_pretty(&legacy).expect("serialize legacy settings"),
        )
        .expect("write legacy settings");

        let settings = load_settings().expect("load settings");

        assert!(
            settings.subscriptions.is_empty(),
            "runtime settings should start empty even if legacy settings exist"
        );
    }

    #[test]
    fn load_settings_initializes_empty_runtime_settings() {
        let _env = TestEnv::new("empty-runtime-settings");

        let settings = load_settings().expect("load settings");

        assert!(settings.subscriptions.is_empty());
    }

    #[test]
    fn save_settings_replaces_existing_file() {
        let _env = TestEnv::new("replace-existing-settings");
        save_settings(&AppSettings::default()).expect("write initial settings");
        let mut updated = AppSettings::default();
        updated.mihomo_path = "updated-mihomo.exe".to_string();

        save_settings(&updated).expect("replace settings");

        assert_eq!(
            load_settings().expect("load replaced settings").mihomo_path,
            "updated-mihomo.exe"
        );
    }

    #[test]
    fn load_settings_backs_up_corrupt_file_before_recovery() {
        let env = TestEnv::new("corrupt-settings-backup");
        let path = runtime_paths::settings_path().expect("settings path");
        std::fs::write(&path, "{not-json").expect("write corrupt settings");

        let settings = load_settings().expect("recover settings");

        assert!(settings.subscriptions.is_empty());
        let backups = std::fs::read_dir(&env.root)
            .expect("read runtime dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.corrupt-")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read_to_string(backups[0].path()).expect("read backup"),
            "{not-json"
        );
    }
}
