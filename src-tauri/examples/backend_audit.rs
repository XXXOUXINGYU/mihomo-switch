#[path = "../src/commands.rs"]
mod commands;
#[path = "../src/config.rs"]
mod config;
#[path = "../src/latency.rs"]
mod latency;
#[path = "../src/logging.rs"]
mod logging;
#[path = "../src/models.rs"]
mod models;
#[path = "../src/parser.rs"]
mod parser;
#[path = "../src/runner.rs"]
mod runner;
#[path = "../src/runtime_paths.rs"]
mod runtime_paths;
#[path = "../src/settings.rs"]
mod settings;
#[path = "../src/state.rs"]
mod state;
#[path = "../src/traffic.rs"]
mod traffic;

use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    thread,
};

use anyhow::{bail, Context, Result};
use models::{AppSettings, ProxyNode, SubscriptionRecord, UpsertSubscriptionInput};
use state::AppState;
use tauri::AppHandle;
use uuid::Uuid;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct AuditEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
    root: PathBuf,
}

impl AuditEnv {
    fn new(name: &str) -> Result<Self> {
        let guard = env_lock().lock().expect("env lock");
        let root = env::temp_dir().join(format!(
            "mihomo-switch-audit-{name}-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)
            .with_context(|| format!("create audit root: {}", root.display()))?;
        env::set_var("MIHOMO_MANAGER_HOME", &root);
        env::set_var("MIHOMO_MANAGER_SKIP_LEGACY_IMPORT", "1");
        Ok(Self {
            _guard: guard,
            root,
        })
    }
}

impl Drop for AuditEnv {
    fn drop(&mut self) {
        env::remove_var("MIHOMO_MANAGER_HOME");
        env::remove_var("MIHOMO_MANAGER_SKIP_LEGACY_IMPORT");
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_that(condition: bool, message: &str) -> Result<()> {
    if !condition {
        bail!("{message}");
    }
    Ok(())
}

fn audit_mihomo_path() -> Result<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("project root")?
        .join("mihomo.exe");
    if !path.exists() {
        bail!("audit mihomo.exe not found: {}", path.display());
    }
    Ok(path.display().to_string())
}

fn proxy_node(name: &str, node_type: &str, server: &str, port: u16) -> ProxyNode {
    ProxyNode {
        name: name.to_string(),
        node_type: node_type.to_string(),
        server: server.to_string(),
        port,
        uuid: "secret".to_string(),
        tls: matches!(node_type, "trojan" | "vless" | "vmess"),
        ..ProxyNode::default()
    }
}

fn subscription(
    id: &str,
    manual: bool,
    content: &str,
    nodes: Vec<ProxyNode>,
    selected: Vec<usize>,
    port_assignments: &[(&str, u16)],
) -> SubscriptionRecord {
    SubscriptionRecord {
        id: id.to_string(),
        name: id.to_string(),
        url: if manual {
            String::new()
        } else {
            format!("https://{id}.example.dev/sub")
        },
        ua: "clash.meta".to_string(),
        start_port: 10801,
        manual,
        content: content.to_string(),
        nodes,
        selected_node_indices: selected,
        port_assignments: port_assignments
            .iter()
            .map(|(key, value)| ((*key).to_string(), *value))
            .collect(),
        node_remarks: BTreeMap::new(),
    }
}

fn audit_create_manual_subscription() -> Result<()> {
    let _env = AuditEnv::new("create-manual")?;
    let state = AppState::default();
    let payload = commands::create_subscription_impl(
        None::<&AppHandle>,
        UpsertSubscriptionInput {
            name: "Manual Feed".to_string(),
            url: String::new(),
            manual: true,
            content: "vless://11111111-1111-1111-1111-111111111111@example.com:443?security=reality&pbk=test-key&sid=abcd#Manual%20Node".to_string(),
        },
        &state,
    )
    .map_err(|error| anyhow::anyhow!("create manual subscription: {error}"))?;

    let created = payload
        .settings
        .subscriptions
        .iter()
        .find(|item| item.name == "Manual Feed")
        .context("find created subscription")?;
    assert_that(created.manual, "created subscription should be manual")?;
    assert_that(
        created.nodes.len() == 1,
        "manual subscription should parse one node",
    )?;
    assert_that(
        created.nodes[0].name == "Manual Node" && created.nodes[0].node_type == "vless",
        "manual subscription parsed unexpected node",
    )?;
    Ok(())
}

fn audit_bootstrap_initializes_runtime_snapshot() -> Result<()> {
    let _env = AuditEnv::new("bootstrap")?;
    settings::save_settings(&AppSettings::default())?;
    let state = AppState::default();
    let payload =
        commands::bootstrap_impl(&state).map_err(|error| anyhow::anyhow!("bootstrap: {error}"))?;

    assert_that(
        payload.settings.schema_version == 3,
        "bootstrap should normalize schema version",
    )?;
    assert_that(
        payload
            .runtime
            .runtime_dir
            .contains("mihomo-switch-audit-bootstrap")
            || payload.runtime.runtime_dir.contains("MihomoSwitch"),
        "bootstrap should expose runtime dir",
    )?;
    assert_that(
        payload.runtime.mihomo_path.ends_with("mihomo.exe"),
        "bootstrap should expose mihomo path",
    )?;
    assert_that(
        !payload.runtime.mihomo_exists,
        "bootstrap should not pretend an unconfigured mihomo binary exists",
    )?;
    Ok(())
}

fn audit_import_manual_subscription() -> Result<()> {
    let _env = AuditEnv::new("import-manual")?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![subscription(
            "manual-sub",
            true,
            "trojan://secret@example.com:443?sni=example.com#Trojan%20Node",
            Vec::new(),
            Vec::new(),
            &[],
        )],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path: crate::models::default_mihomo_path_string(),
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    let state = AppState::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    let payload = runtime
        .block_on(commands::import_subscription_impl(
            None::<&AppHandle>,
            "manual-sub".to_string(),
            &state,
        ))
        .map_err(|error| anyhow::anyhow!("import manual subscription: {error}"))?;

    let imported = payload
        .settings
        .subscriptions
        .iter()
        .find(|item| item.id == "manual-sub")
        .context("find imported subscription")?;
    assert_that(
        imported.nodes.len() == 1,
        "manual import should yield one node",
    )?;
    assert_that(
        imported.nodes[0].name == "Trojan Node" && imported.nodes[0].node_type == "trojan",
        "manual import parsed unexpected node",
    )?;
    Ok(())
}

fn serve_subscription_once(body: &str) -> Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").context("bind local subscription server")?;
    let address = listener
        .local_addr()
        .context("read subscription server addr")?;
    let response_body = body.to_string();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0_u8; 2048];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    Ok(format!("http://{address}/subscription"))
}

fn audit_import_remote_subscription() -> Result<()> {
    let _env = AuditEnv::new("import-remote")?;
    let remote_url =
        serve_subscription_once("trojan://secret@example.com:443?sni=example.com#Remote%20Trojan")?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![SubscriptionRecord {
            id: "remote-sub".to_string(),
            name: "Remote Feed".to_string(),
            url: remote_url,
            ua: "mihomo-switch".to_string(),
            start_port: 10801,
            manual: false,
            content: String::new(),
            nodes: Vec::new(),
            selected_node_indices: Vec::new(),
            port_assignments: BTreeMap::new(),
            node_remarks: BTreeMap::new(),
        }],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path: crate::models::default_mihomo_path_string(),
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    let state = AppState::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    let payload = runtime
        .block_on(commands::import_subscription_impl(
            None::<&AppHandle>,
            "remote-sub".to_string(),
            &state,
        ))
        .map_err(|error| anyhow::anyhow!("import remote subscription: {error}"))?;

    let imported = payload
        .settings
        .subscriptions
        .iter()
        .find(|item| item.id == "remote-sub")
        .context("find imported remote subscription")?;
    assert_that(
        imported.nodes.len() == 1,
        "remote import should yield one node",
    )?;
    assert_that(
        imported.nodes[0].name == "Remote Trojan" && imported.nodes[0].node_type == "trojan",
        "remote import parsed unexpected node",
    )?;
    Ok(())
}

fn audit_save_selection_reassigns_conflicts() -> Result<()> {
    let _env = AuditEnv::new("save-selection")?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![
            subscription(
                "sub-a",
                false,
                "",
                vec![proxy_node("A", "vmess", "a.example.com", 443)],
                vec![0],
                &[("0", 10801)],
            ),
            subscription(
                "sub-b",
                false,
                "",
                vec![proxy_node("B", "trojan", "b.example.com", 8443)],
                Vec::new(),
                &[],
            ),
        ],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path: crate::models::default_mihomo_path_string(),
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    let state = AppState::default();
    let mut requested_ports = BTreeMap::new();
    requested_ports.insert("0".to_string(), 10801);
    let payload = commands::save_selection_impl(
        None::<&AppHandle>,
        "sub-b".to_string(),
        vec![0],
        requested_ports,
        &state,
    )
    .map_err(|error| anyhow::anyhow!("save selection: {error}"))?;

    let updated = payload
        .settings
        .subscriptions
        .iter()
        .find(|item| item.id == "sub-b")
        .context("find updated subscription")?;
    assert_that(
        updated.selected_node_indices == vec![0],
        "selected indices should persist",
    )?;
    assert_that(
        updated
            .port_assignments
            .get("0")
            .copied()
            .unwrap_or_default()
            != 10801,
        "conflicting port should be reassigned",
    )?;
    assert_that(
        Path::new(&payload.runtime.config_path).exists(),
        "pool config should be generated",
    )?;
    Ok(())
}

fn audit_update_manual_subscription_reparses_and_clears_selection() -> Result<()> {
    let _env = AuditEnv::new("update-manual")?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![subscription(
            "manual-sub",
            true,
            "trojan://secret@example.com:443?sni=example.com#Old%20Node",
            vec![proxy_node("Old Node", "trojan", "example.com", 443)],
            vec![0],
            &[("0", 10801)],
        )],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path: crate::models::default_mihomo_path_string(),
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    let state = AppState::default();
    let payload = commands::update_subscription_impl(
        None::<&AppHandle>,
        "manual-sub".to_string(),
        UpsertSubscriptionInput {
            name: "Manual Feed Updated".to_string(),
            url: String::new(),
            manual: true,
            content: "vless://11111111-1111-1111-1111-111111111111@example.com:443?security=reality&pbk=test-key&sid=abcd#Updated%20Node".to_string(),
        },
        &state,
    )
    .map_err(|error| anyhow::anyhow!("update manual subscription: {error}"))?;

    let updated = payload
        .settings
        .subscriptions
        .iter()
        .find(|item| item.id == "manual-sub")
        .context("find updated manual subscription")?;
    assert_that(
        updated.name == "Manual Feed Updated",
        "manual name should update",
    )?;
    assert_that(
        updated.nodes.len() == 1,
        "manual update should reparse nodes",
    )?;
    assert_that(
        updated.nodes[0].name == "Updated Node" && updated.nodes[0].node_type == "vless",
        "manual update parsed unexpected node",
    )?;
    assert_that(
        updated.selected_node_indices.is_empty() && updated.port_assignments.is_empty(),
        "manual update should clear selection and port assignments",
    )?;
    Ok(())
}

fn audit_update_remote_subscription_source_clears_imported_nodes() -> Result<()> {
    let _env = AuditEnv::new("update-remote-source")?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![subscription(
            "remote-sub",
            false,
            "",
            vec![proxy_node("Old Remote", "trojan", "old.example.com", 443)],
            vec![0],
            &[("0", 10801)],
        )],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path: crate::models::default_mihomo_path_string(),
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    config::write_pool_config(&settings::load_settings()?)?;

    let state = AppState::default();
    let payload = commands::update_subscription_impl(
        None::<&AppHandle>,
        "remote-sub".to_string(),
        UpsertSubscriptionInput {
            name: "Remote Feed Updated".to_string(),
            url: "https://new.example.dev/sub".to_string(),
            manual: false,
            content: String::new(),
        },
        &state,
    )
    .map_err(|error| anyhow::anyhow!("update remote subscription source: {error}"))?;

    let updated = payload
        .settings
        .subscriptions
        .iter()
        .find(|item| item.id == "remote-sub")
        .context("find updated remote subscription")?;
    assert_that(
        updated.name == "Remote Feed Updated",
        "remote name should update",
    )?;
    assert_that(
        updated.url == "https://new.example.dev/sub",
        "remote URL should update",
    )?;
    assert_that(
        updated.nodes.is_empty()
            && updated.selected_node_indices.is_empty()
            && updated.port_assignments.is_empty(),
        "remote source change should clear imported nodes, selection and ports",
    )?;
    assert_that(
        !Path::new(&payload.runtime.config_path).exists(),
        "pool config should be removed after remote source clears selection",
    )?;
    Ok(())
}

fn audit_delete_selected_nodes_persists() -> Result<()> {
    let _env = AuditEnv::new("delete-nodes")?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![subscription(
            "sub-a",
            false,
            "",
            vec![
                proxy_node("A", "vmess", "a.example.com", 443),
                proxy_node("B", "trojan", "b.example.com", 8443),
                proxy_node("C", "ss", "c.example.com", 9443),
            ],
            vec![0, 2],
            &[("0", 10801), ("2", 10803)],
        )],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path: crate::models::default_mihomo_path_string(),
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    let state = AppState::default();
    let payload = commands::delete_selected_nodes_impl(
        None::<&AppHandle>,
        "sub-a".to_string(),
        vec![0, 2],
        &state,
    )
    .map_err(|error| anyhow::anyhow!("delete selected nodes: {error}"))?;

    let updated = payload
        .settings
        .subscriptions
        .iter()
        .find(|item| item.id == "sub-a")
        .context("find updated subscription after delete")?;
    assert_that(updated.nodes.len() == 1, "two nodes should be deleted")?;
    assert_that(
        updated.selected_node_indices.is_empty() && updated.port_assignments.is_empty(),
        "selection and ports should be cleared after delete",
    )?;
    Ok(())
}

fn audit_delete_subscription_removes_pool_config() -> Result<()> {
    let _env = AuditEnv::new("delete-subscription")?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![subscription(
            "sub-a",
            false,
            "",
            vec![proxy_node("A", "vmess", "a.example.com", 443)],
            vec![0],
            &[("0", 10801)],
        )],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path: crate::models::default_mihomo_path_string(),
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    config::write_pool_config(&settings::load_settings()?)?;
    let state = AppState::default();
    let payload =
        commands::delete_subscription_impl(None::<&AppHandle>, "sub-a".to_string(), &state)
            .map_err(|error| anyhow::anyhow!("delete subscription: {error}"))?;

    assert_that(
        payload.settings.subscriptions.is_empty(),
        "subscription should be deleted",
    )?;
    assert_that(
        !Path::new(&payload.runtime.config_path).exists(),
        "pool config should be removed after deleting last selected subscription",
    )?;
    Ok(())
}

fn audit_reorder_subscriptions_persists() -> Result<()> {
    let _env = AuditEnv::new("reorder-subscriptions")?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![
            subscription("sub-a", false, "", Vec::new(), Vec::new(), &[]),
            subscription("sub-b", false, "", Vec::new(), Vec::new(), &[]),
            subscription("sub-c", false, "", Vec::new(), Vec::new(), &[]),
        ],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path: crate::models::default_mihomo_path_string(),
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    let state = AppState::default();
    let payload = commands::reorder_subscriptions_impl(
        vec![
            "sub-c".to_string(),
            "sub-a".to_string(),
            "sub-b".to_string(),
        ],
        &state,
    )
    .map_err(|error| anyhow::anyhow!("reorder subscriptions: {error}"))?;

    let ordered_ids = payload
        .settings
        .subscriptions
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();
    assert_that(
        ordered_ids == vec!["sub-c", "sub-a", "sub-b"],
        "reordered subscriptions should match requested order",
    )?;

    let persisted_ids = settings::load_settings()?
        .subscriptions
        .iter()
        .map(|item| item.id.as_str().to_string())
        .collect::<Vec<_>>();
    assert_that(
        persisted_ids == vec!["sub-c", "sub-a", "sub-b"],
        "reordered subscriptions should persist to settings",
    )?;
    Ok(())
}

fn audit_save_subconverter_persists() -> Result<()> {
    let _env = AuditEnv::new("save-subconverter")?;
    settings::save_settings(&AppSettings::default())?;
    let state = AppState::default();
    let payload =
        commands::save_subconverter_impl("https://subconverter.example.dev".to_string(), &state)
            .map_err(|error| anyhow::anyhow!("save subconverter: {error}"))?;

    assert_that(
        payload.settings.subconverter == "https://subconverter.example.dev",
        "subconverter should update in payload",
    )?;
    assert_that(
        settings::load_settings()?.subconverter == "https://subconverter.example.dev",
        "subconverter should persist to settings",
    )?;
    Ok(())
}

fn audit_start_requires_selected_node() -> Result<()> {
    let _env = AuditEnv::new("start-empty")?;
    settings::save_settings(&AppSettings::default())?;
    let state = AppState::default();
    let error = commands::start_mihomo_impl(None::<&AppHandle>, &state)
        .expect_err("start should reject empty config");
    assert_that(
        error == "请先启用至少一个节点后再启动",
        "unexpected empty-start error",
    )?;
    Ok(())
}

fn audit_runner_headless_start_stop_cycle() -> Result<()> {
    let _env = AuditEnv::new("runner-cycle")?;
    let mihomo_path = audit_mihomo_path()?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![subscription(
            "sub-a",
            false,
            "",
            vec![proxy_node("Audit Trojan", "trojan", "example.com", 443)],
            vec![0],
            &[("0", 18881)],
        )],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path,
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    let state = AppState::default();
    let started = commands::start_mihomo_headless_impl(&state)
        .map_err(|error| anyhow::anyhow!("start mihomo headless impl: {error}"))?;
    let running_after_start = started.runtime.running;
    let stopped = commands::stop_mihomo_headless_impl(&state)
        .map_err(|error| anyhow::anyhow!("stop mihomo headless impl: {error}"))?;

    assert_that(
        running_after_start,
        "mihomo should still be running after headless start",
    )?;
    assert_that(
        !stopped.runtime.running,
        "mihomo should stop after headless stop",
    )?;
    Ok(())
}

fn audit_latency_headless_returns_result() -> Result<()> {
    let _env = AuditEnv::new("latency-headless")?;
    let mihomo_path = audit_mihomo_path()?;
    settings::save_settings(&AppSettings {
        schema_version: 2,
        subscriptions: vec![subscription(
            "sub-a",
            false,
            "",
            vec![proxy_node("Audit Trojan", "trojan", "example.com", 443)],
            vec![0],
            &[("0", 18881)],
        )],
        port_slots: Vec::new(),
        slots_migrated: false,
        subconverter: String::new(),
        mihomo_path,
        local_proxy_enabled: false,
        local_proxy_url: crate::models::default_local_proxy_url(),
    })?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    let results = runtime
        .block_on(commands::test_latency_headless_impl(
            "sub-a".to_string(),
            vec![0],
        ))
        .map_err(|error| anyhow::anyhow!("test latency headless impl: {error}"))?;

    assert_that(results.len() == 1, "latency test should return one result")?;
    assert_that(
        results[0].sub_id == "sub-a" && results[0].node_index == 0,
        "latency result should keep sub id and node index",
    )?;
    assert_that(
        !results[0].result.trim().is_empty(),
        "latency result should not be empty",
    )?;
    Ok(())
}

pub fn run_all_checks() -> Vec<(&'static str, Result<()>)> {
    let checks: [(&str, fn() -> Result<()>); 14] = [
        (
            "bootstrap_initializes_runtime_snapshot",
            audit_bootstrap_initializes_runtime_snapshot,
        ),
        (
            "create_manual_subscription",
            audit_create_manual_subscription,
        ),
        (
            "import_manual_subscription",
            audit_import_manual_subscription,
        ),
        (
            "import_remote_subscription",
            audit_import_remote_subscription,
        ),
        (
            "save_selection_reassigns_conflicts",
            audit_save_selection_reassigns_conflicts,
        ),
        (
            "update_manual_subscription_reparses_and_clears_selection",
            audit_update_manual_subscription_reparses_and_clears_selection,
        ),
        (
            "update_remote_subscription_source_clears_imported_nodes",
            audit_update_remote_subscription_source_clears_imported_nodes,
        ),
        (
            "delete_selected_nodes_persists",
            audit_delete_selected_nodes_persists,
        ),
        (
            "delete_subscription_removes_pool_config",
            audit_delete_subscription_removes_pool_config,
        ),
        (
            "reorder_subscriptions_persists",
            audit_reorder_subscriptions_persists,
        ),
        (
            "save_subconverter_persists",
            audit_save_subconverter_persists,
        ),
        (
            "start_requires_selected_node",
            audit_start_requires_selected_node,
        ),
        (
            "runner_headless_start_stop_cycle",
            audit_runner_headless_start_stop_cycle,
        ),
        (
            "latency_headless_returns_result",
            audit_latency_headless_returns_result,
        ),
    ];

    checks
        .into_iter()
        .map(|(name, check)| (name, check()))
        .collect()
}

#[cfg(not(test))]
fn main() {
    let mut failed = false;
    for (name, result) in run_all_checks() {
        match result {
            Ok(()) => println!("PASS {name}"),
            Err(error) => {
                failed = true;
                eprintln!("FAIL {name}: {error:#}");
            }
        }
    }

    if failed {
        std::process::exit(1);
    }
}
