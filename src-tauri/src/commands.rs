use std::collections::BTreeMap;

use anyhow::Result;
use tauri::{AppHandle, Runtime, State};

use crate::{
    config, latency,
    logging::emit_log,
    models::{
        AppSettings, BootstrapPayload, LatencyResult, NodeTrafficPanel, NodeTrafficSnapshot,
        PortSlotBatchBindingInput, PortSlotBindingInput, PortSlotInput, PortTrafficReport,
        PortValidation, RuntimeSnapshot, UpsertSubscriptionInput,
    },
    parser, runtime_paths,
    settings::{self, find_subscription, find_subscription_mut},
    state::AppState,
    traffic,
};

fn snapshot(state: &AppState) -> Result<RuntimeSnapshot> {
    let settings = settings::load_settings()?;
    let mihomo_path = runtime_paths::resolve_mihomo_path(&settings.mihomo_path);
    Ok(RuntimeSnapshot {
        config_path: runtime_paths::pool_config_path()?.display().to_string(),
        mihomo_path: mihomo_path.display().to_string(),
        mihomo_exists: mihomo_path.is_file(),
        runtime_dir: runtime_paths::runtime_dir().display().to_string(),
        running: state.runner.is_running(),
    })
}

fn payload_from_settings(state: &AppState, settings: AppSettings) -> Result<BootstrapPayload> {
    let slots = settings::build_slot_views(&settings);
    Ok(BootstrapPayload {
        settings,
        slots,
        runtime: snapshot(state)?,
        migration: None,
    })
}

fn bootstrap_payload(state: &AppState) -> Result<BootstrapPayload> {
    let mut settings = settings::load_settings()?;
    let migration = settings::ensure_port_slots(&mut settings);
    if migration.is_some() {
        settings::save_settings(&settings)?;
    }
    let slots = settings::build_slot_views(&settings);
    // Only surface a migration banner when something actually moved.
    let migration =
        migration.filter(|report| report.created_slots > 0 || !report.messages.is_empty());
    let mut payload = payload_from_settings(state, settings)?;
    payload.slots = slots;
    payload.migration = migration;
    Ok(payload)
}

/// Run a settings mutation, persist + hot-apply it, and return a fresh payload.
fn mutate_and_persist<R: Runtime, F>(
    app: Option<&AppHandle<R>>,
    state: &AppState,
    mutate: F,
) -> Result<BootstrapPayload, String>
where
    F: FnOnce(&mut AppSettings) -> Result<(), String>,
{
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    mutate(&mut settings_data)?;
    persist_runtime_settings(app, state, &settings_data).map_err(|error| error.to_string())?;
    payload_from_settings(state, settings_data).map_err(|error| error.to_string())
}

pub(crate) fn bootstrap_impl(state: &AppState) -> Result<BootstrapPayload, String> {
    let payload = bootstrap_payload(state).map_err(|error| error.to_string())?;
    config::write_pool_config(&payload.settings).map_err(|error| error.to_string())?;
    Ok(payload)
}

fn persist_runtime_settings<R: Runtime>(
    app: Option<&AppHandle<R>>,
    state: &AppState,
    settings_data: &crate::models::AppSettings,
) -> Result<()> {
    let previous_settings = settings::load_settings()?;
    let prepared_config = config::prepare_pool_config(settings_data)?;
    let has_active_slots = prepared_config.path().is_some();
    let was_running = state.runner.is_running();

    if let Some(app) = app {
        if was_running && has_active_slots {
            emit_log(app, "info", "检测到配置变更，正在验证并无感应用新配置");
            let candidate = prepared_config.path().expect("active config path");
            if let Err(reload_error) = state.runner.reload_from(app, candidate) {
                emit_log(
                    app,
                    "warn",
                    format!("无感热加载失败，正在自动重启核心应用新配置: {reload_error}"),
                );
                if let Err(restart_error) = state.runner.restart_from(app, settings_data, candidate)
                {
                    restore_running_settings(app, state, &previous_settings).map_err(
                        |rollback_error| {
                            anyhow::anyhow!(
                                "热加载失败: {reload_error}; 使用新配置重启失败: {restart_error}; 恢复旧运行配置失败: {rollback_error}"
                            )
                        },
                    )?;
                    return Err(anyhow::anyhow!(
                        "热加载失败: {reload_error}; 使用新配置重启失败: {restart_error}; 已恢复旧运行配置"
                    ));
                }
                emit_log(app, "info", "mihomo 已通过自动重启应用新配置");
            }
        }
    }

    if let Err(error) = settings::save_settings(settings_data) {
        if let Some(app) = app {
            if was_running && has_active_slots {
                restore_running_settings(app, state, &previous_settings).map_err(
                    |rollback_error| {
                        anyhow::anyhow!(
                            "保存新设置失败: {error}; 恢复上一运行配置失败: {rollback_error}"
                        )
                    },
                )?;
            }
        }
        return Err(error);
    }

    if let Err(error) = prepared_config.commit() {
        restore_persisted_settings(&previous_settings).map_err(|rollback_error| {
            anyhow::anyhow!("提交新运行配置失败: {error}; 恢复上一版本文件失败: {rollback_error}")
        })?;
        if let Some(app) = app {
            if was_running && has_active_slots {
                restore_running_settings(app, state, &previous_settings).map_err(
                    |rollback_error| {
                        anyhow::anyhow!(
                            "提交新运行配置失败: {error}; 文件已恢复，但恢复上一运行配置失败: {rollback_error}"
                        )
                    },
                )?;
            }
            emit_log(app, "warn", "新配置提交失败，已恢复上一版本");
        }
        return Err(error);
    }

    let Some(app) = app else {
        return Ok(());
    };
    if !was_running {
        return Ok(());
    }
    if has_active_slots {
        emit_log(app, "info", "mihomo 已无感应用并提交新配置");
        return Ok(());
    }

    emit_log(app, "info", "运行中配置已清空，正在停止 mihomo");
    if let Err(error) = state.runner.stop(app) {
        restore_persisted_settings(&previous_settings).map_err(|rollback_error| {
            anyhow::anyhow!("停止 mihomo 失败: {error}; 恢复上一版本文件失败: {rollback_error}")
        })?;
        restore_running_settings(app, state, &previous_settings).map_err(|rollback_error| {
            anyhow::anyhow!(
                "停止 mihomo 失败: {error}; 文件已恢复，但恢复上一运行配置失败: {rollback_error}"
            )
        })?;
        return Err(error);
    }

    Ok(())
}

fn restore_persisted_settings(previous_settings: &AppSettings) -> Result<()> {
    settings::save_settings(previous_settings)?;
    config::write_pool_config(previous_settings)?;
    Ok(())
}

fn restore_running_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    previous_settings: &AppSettings,
) -> Result<()> {
    if config::collect_active_slots(previous_settings).is_empty() {
        return state.runner.stop(app);
    }
    if state.runner.is_running() && state.runner.reload(app).is_ok() {
        return Ok(());
    }
    state.runner.restart(app, previous_settings)
}

pub(crate) fn create_subscription_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    input: UpsertSubscriptionInput,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let manual = input.manual;
    let content = input.content.clone();
    let created = settings::create_subscription(&mut settings_data, input)
        .map_err(|error| error.to_string())?;
    if manual {
        let parsed = parser::detect_and_parse(&content).map_err(|error| error.to_string())?;
        if let Some(target) = find_subscription_mut(&mut settings_data, &created.id) {
            target.nodes = parsed;
            target.node_remarks.clear();
        }
        settings::recalculate_ports(&mut settings_data, &created.id);
        if let Some(app) = app {
            emit_log(app, "info", format!("已加载手动订阅: {}", created.name));
        }
    }
    settings::save_settings(&settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

/// UI-facing URL subscription creation: fetch and parse before persisting so a
/// failed request never leaves an empty subscription record behind.
pub(crate) async fn create_subscription_with_import_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    input: UpsertSubscriptionInput,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    if input.manual {
        return create_subscription_impl(app, input, state);
    }

    let initial_settings = settings::load_settings().map_err(|error| error.to_string())?;
    if let Some(app) = app {
        emit_log(app, "info", format!("正在拉取订阅: {}", input.name));
    }
    let content = if let Some(app) = app {
        parser::fetch_subscription(
            app,
            &input.url,
            Some(&crate::models::default_ua()),
            &initial_settings.subconverter,
            initial_settings.local_proxy_enabled,
            &initial_settings.local_proxy_url,
        )
        .await
    } else {
        parser::fetch_subscription_headless(
            &input.url,
            Some(&crate::models::default_ua()),
            &initial_settings.subconverter,
            initial_settings.local_proxy_enabled,
            &initial_settings.local_proxy_url,
        )
        .await
    }
    .map_err(|error| error.to_string())?;
    let parsed = parser::detect_and_parse(&content).map_err(|error| error.to_string())?;
    let imported_count = parsed.len();

    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let created = settings::create_subscription(&mut settings_data, input)
        .map_err(|error| error.to_string())?;
    if let Some(target) = find_subscription_mut(&mut settings_data, &created.id) {
        target.nodes = parsed;
    }
    settings::save_settings(&settings_data).map_err(|error| error.to_string())?;
    if let Some(app) = app {
        emit_log(
            app,
            "info",
            format!("订阅已创建并导入: {} 个节点", imported_count),
        );
    }
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn update_subscription_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    sub_id: String,
    input: UpsertSubscriptionInput,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let should_apply_runtime = input.manual;
    let manual_content = input.content.clone();
    let update_outcome = settings::update_subscription(&mut settings_data, &sub_id, input)
        .map_err(|error| error.to_string())?;
    if should_apply_runtime {
        let parsed =
            parser::detect_and_parse(&manual_content).map_err(|error| error.to_string())?;
        if let Some(target) = find_subscription_mut(&mut settings_data, &sub_id) {
            target.nodes = parsed;
            target.node_remarks.clear();
        }
        settings::recalculate_ports(&mut settings_data, &sub_id);
        if let (Some(app), Some(sub)) = (app, find_subscription(&settings_data, &sub_id)) {
            emit_log(app, "info", format!("已更新手动订阅: {}", sub.name));
        }
    } else if update_outcome.source_changed {
        if let (Some(app), Some(sub)) = (app, find_subscription(&settings_data, &sub_id)) {
            emit_log(
                app,
                "info",
                format!("已更新订阅来源: {}，请重新导入节点", sub.name),
            );
        }
    }
    if should_apply_runtime || update_outcome.cleared_nodes {
        persist_runtime_settings(app, state, &settings_data).map_err(|error| error.to_string())?;
    } else {
        settings::save_settings(&settings_data).map_err(|error| error.to_string())?;
    }
    bootstrap_payload(state).map_err(|error| error.to_string())
}

/// UI-facing subscription update. When a URL source changes, fetch it first
/// and only replace the persisted source/nodes after the new data is valid.
pub(crate) async fn update_subscription_with_import_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    sub_id: String,
    input: UpsertSubscriptionInput,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    if input.manual {
        return update_subscription_impl(app, sub_id, input, state);
    }

    let initial_settings = settings::load_settings().map_err(|error| error.to_string())?;
    let Some(initial_sub) = find_subscription(&initial_settings, &sub_id).cloned() else {
        return Err("目标订阅不存在".to_string());
    };
    let next_url = input.url.trim();
    let source_changed = initial_sub.manual || initial_sub.url != next_url;
    if !source_changed {
        return update_subscription_impl(app, sub_id, input, state);
    }

    if let Some(app) = app {
        emit_log(app, "info", format!("正在拉取订阅: {}", input.name));
    }
    let content = if let Some(app) = app {
        parser::fetch_subscription(
            app,
            next_url,
            Some(&initial_sub.ua),
            &initial_settings.subconverter,
            initial_settings.local_proxy_enabled,
            &initial_settings.local_proxy_url,
        )
        .await
    } else {
        parser::fetch_subscription_headless(
            next_url,
            Some(&initial_sub.ua),
            &initial_settings.subconverter,
            initial_settings.local_proxy_enabled,
            &initial_settings.local_proxy_url,
        )
        .await
    }
    .map_err(|error| error.to_string())?;
    let parsed = parser::detect_and_parse(&content).map_err(|error| error.to_string())?;
    let imported_count = parsed.len();

    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let Some(latest_sub) = find_subscription(&settings_data, &sub_id) else {
        return Err("订阅在更新期间已被删除，已放弃更新结果".to_string());
    };
    if latest_sub.manual != initial_sub.manual || latest_sub.url != initial_sub.url {
        return Err("订阅来源在更新期间已发生变化，已放弃旧的更新结果".to_string());
    }

    settings::update_subscription(&mut settings_data, &sub_id, input)
        .map_err(|error| error.to_string())?;
    if let Some(target) = find_subscription_mut(&mut settings_data, &sub_id) {
        target.nodes = parsed;
    }
    settings::recalculate_ports(&mut settings_data, &sub_id);
    persist_runtime_settings(app, state, &settings_data).map_err(|error| error.to_string())?;
    if let Some(app) = app {
        emit_log(
            app,
            "info",
            format!("订阅已更新并导入: {} 个节点", imported_count),
        );
    }
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn delete_subscription_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    sub_id: String,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    settings::delete_subscription(&mut settings_data, &sub_id);
    persist_runtime_settings(app, state, &settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn delete_selected_nodes_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    sub_id: String,
    node_indices: Vec<usize>,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let removed = settings::delete_selected_nodes(&mut settings_data, &sub_id, &node_indices);
    if removed == 0 {
        return bootstrap_payload(state).map_err(|error| error.to_string());
    }

    if let Some(app) = app {
        emit_log(app, "info", format!("已删除 {removed} 个节点"));
    }
    persist_runtime_settings(app, state, &settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) async fn import_subscription_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    sub_id: String,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let initial_settings = settings::load_settings().map_err(|error| error.to_string())?;
    let Some(sub) = find_subscription(&initial_settings, &sub_id).cloned() else {
        return Err("目标订阅不存在".to_string());
    };

    let content = if sub.manual {
        sub.content.clone()
    } else if let Some(app) = app {
        emit_log(app, "info", format!("正在拉取订阅: {}", sub.name));
        parser::fetch_subscription(
            app,
            &sub.url,
            Some(&sub.ua),
            &initial_settings.subconverter,
            initial_settings.local_proxy_enabled,
            &initial_settings.local_proxy_url,
        )
        .await
        .map_err(|error| error.to_string())?
    } else {
        parser::fetch_subscription_headless(
            &sub.url,
            Some(&sub.ua),
            &initial_settings.subconverter,
            initial_settings.local_proxy_enabled,
            &initial_settings.local_proxy_url,
        )
        .await
        .map_err(|error| error.to_string())?
    };

    let parsed = parser::detect_and_parse(&content).map_err(|error| error.to_string())?;
    // Fetching can take over a minute. Merge into a fresh settings snapshot so
    // changes made while the request was in flight are never overwritten.
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let Some(latest_sub) = find_subscription(&settings_data, &sub_id) else {
        return Err("订阅在更新期间已被删除，已放弃导入结果".to_string());
    };
    let source_unchanged = latest_sub.manual == sub.manual
        && latest_sub.ua == sub.ua
        && if sub.manual {
            latest_sub.content == sub.content
        } else {
            latest_sub.url == sub.url
        };
    if !source_unchanged {
        return Err("订阅来源在更新期间已发生变化，已放弃旧的导入结果".to_string());
    }
    if let Some(target) = find_subscription_mut(&mut settings_data, &sub_id) {
        target.nodes = parsed;
        target.node_remarks.clear();
    }
    settings::recalculate_ports(&mut settings_data, &sub_id);
    let imported_count = find_subscription(&settings_data, &sub_id)
        .map(|sub| sub.nodes.len())
        .unwrap_or_default();
    if let Some(app) = app {
        emit_log(app, "info", format!("导入完成: {imported_count} 个节点"));
    }
    persist_runtime_settings(app, state, &settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn save_selection_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    sub_id: String,
    selected_node_indices: Vec<usize>,
    port_assignments: BTreeMap<String, u16>,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let requested_ports = port_assignments.clone();
    let previous_ports = find_subscription(&settings_data, &sub_id)
        .map(|sub| sub.port_assignments.clone())
        .unwrap_or_default();
    if let Some(target) = find_subscription_mut(&mut settings_data, &sub_id) {
        target.selected_node_indices = selected_node_indices;
        target.port_assignments = port_assignments;
    }
    settings::recalculate_ports_with_previous(
        &mut settings_data,
        &sub_id,
        Some(&previous_ports),
        state.runner.is_running(),
    );
    if let Some(target) = find_subscription(&settings_data, &sub_id) {
        if target.port_assignments != requested_ports {
            if let Some(app) = app {
                emit_log(app, "warn", "部分端口无效、冲突或已被占用，已自动重新分配");
            }
        }
    }
    persist_runtime_settings(app, state, &settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn save_node_remark_impl<R: Runtime>(
    _app: Option<&AppHandle<R>>,
    sub_id: String,
    node_index: usize,
    remark: String,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    if !settings::save_node_remark(&mut settings_data, &sub_id, node_index, remark) {
        return Err("目标节点不存在".to_string());
    }
    settings::save_settings(&settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn reorder_subscriptions_impl(
    ordered_ids: Vec<String>,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    settings::reorder_subscriptions(&mut settings_data, &ordered_ids);
    settings::save_settings(&settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn save_subconverter_impl(
    url: String,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    settings_data.subconverter = url;
    settings::save_settings(&settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn save_proxy_settings_impl(
    enabled: bool,
    url: String,
    mihomo_path: String,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let proxy_url = if url.trim().is_empty() {
        crate::models::default_local_proxy_url()
    } else {
        url.trim().to_string()
    };
    if enabled {
        let parsed =
            url::Url::parse(&proxy_url).map_err(|error| format!("本地代理地址无效：{error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err("本地代理地址仅支持完整的 http:// 或 https:// 地址".to_string());
        }
    }
    let mut settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    settings_data.local_proxy_enabled = enabled;
    settings_data.local_proxy_url = proxy_url;
    settings_data.mihomo_path = if mihomo_path.trim().is_empty() {
        crate::models::default_mihomo_path_string()
    } else {
        mihomo_path.trim().to_string()
    };
    settings::save_settings(&settings_data).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

/// Collect human-readable reasons for enabled slots that cannot run, so the UI
/// can show a single consolidated message before/while starting.
fn collect_start_blockers(settings: &AppSettings) -> Vec<String> {
    settings::build_slot_views(settings)
        .into_iter()
        .filter(|view| view.enabled && view.state != "valid")
        .map(|view| {
            let reason = match view.state.as_str() {
                "unbound" => "未绑定节点".to_string(),
                _ => view
                    .invalid_reason
                    .unwrap_or_else(|| "节点不可用".to_string()),
            };
            format!("{}（{}）", view.name, reason)
        })
        .collect()
}

pub(crate) fn start_mihomo_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let config_path =
        config::write_pool_config(&settings_data).map_err(|error| error.to_string())?;
    let blockers = collect_start_blockers(&settings_data);
    if config_path.is_none() {
        if blockers.is_empty() {
            return Err("请先创建并启用至少一个绑定有效节点的端口".to_string());
        }
        return Err(format!(
            "以下端口无法启动，请先修复后再试：\n{}",
            blockers.join("\n")
        ));
    }
    let Some(app) = app else {
        return Err("缺少应用句柄，无法启动 mihomo".to_string());
    };
    if !blockers.is_empty() {
        emit_log(
            app,
            "warn",
            format!("{} 个端口已跳过：{}", blockers.len(), blockers.join("；")),
        );
    }
    state
        .runner
        .start(app, &settings_data)
        .map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

#[allow(dead_code)]
pub(crate) fn start_mihomo_headless_impl(state: &AppState) -> Result<BootstrapPayload, String> {
    let _settings_operation = state.settings_guard();
    let settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let config_path =
        config::write_pool_config(&settings_data).map_err(|error| error.to_string())?;
    if config_path.is_none() {
        return Err("请先创建并启用至少一个绑定有效节点的端口".to_string());
    }
    state
        .runner
        .start_headless(&settings_data)
        .map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

pub(crate) fn stop_mihomo_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    let Some(app) = app else {
        return Err("缺少应用句柄，无法停止 mihomo".to_string());
    };
    state.runner.stop(app).map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

#[allow(dead_code)]
pub(crate) fn stop_mihomo_headless_impl(state: &AppState) -> Result<BootstrapPayload, String> {
    state
        .runner
        .stop_headless()
        .map_err(|error| error.to_string())?;
    bootstrap_payload(state).map_err(|error| error.to_string())
}

#[allow(dead_code)]
pub(crate) async fn test_latency_headless_impl(
    sub_id: String,
    node_indices: Vec<usize>,
) -> Result<Vec<LatencyResult>, String> {
    let settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let Some(sub) = find_subscription(&settings_data, &sub_id) else {
        return Err("目标订阅不存在".to_string());
    };

    let nodes = node_indices
        .into_iter()
        .filter_map(|index| sub.nodes.get(index).cloned().map(|node| (index, node)))
        .collect::<Vec<_>>();

    if nodes.is_empty() {
        return Err("当前没有可测速的节点".to_string());
    }

    latency::test_nodes_latency_headless(&sub_id, &nodes, 5_000)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) fn cancel_latency_impl(state: &AppState) {
    state.latency.cancel();
}

pub(crate) fn node_traffic_snapshot_impl(
    sub_id: String,
    node_index: usize,
    state: &AppState,
) -> Result<NodeTrafficSnapshot, String> {
    let settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    if !state.runner.is_running() {
        return traffic::analyze_connections_payload(
            &settings_data,
            &sub_id,
            node_index,
            &serde_json::json!({}),
            false,
        )
        .map_err(|error| error.to_string());
    }

    let Some((controller_port, controller_secret)) = state.runner.controller_access() else {
        return traffic::analyze_connections_payload(
            &settings_data,
            &sub_id,
            node_index,
            &serde_json::json!({}),
            false,
        )
        .map_err(|error| error.to_string());
    };

    traffic::fetch_node_traffic_snapshot(
        &settings_data,
        controller_port,
        &controller_secret,
        &sub_id,
        node_index,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn node_traffic_panel_impl(
    sub_id: String,
    node_index: usize,
    state: &AppState,
) -> Result<NodeTrafficPanel, String> {
    let settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    if let Some(panel) = state.traffic.panel(&sub_id, node_index) {
        return Ok(panel);
    }

    let snapshot = if !state.runner.is_running() {
        traffic::analyze_connections_payload(
            &settings_data,
            &sub_id,
            node_index,
            &serde_json::json!({}),
            false,
        )
        .map_err(|error| error.to_string())?
    } else if let Some((controller_port, controller_secret)) = state.runner.controller_access() {
        traffic::fetch_node_traffic_snapshot(
            &settings_data,
            controller_port,
            &controller_secret,
            &sub_id,
            node_index,
        )
        .map_err(|error| error.to_string())?
    } else {
        traffic::analyze_connections_payload(
            &settings_data,
            &sub_id,
            node_index,
            &serde_json::json!({}),
            false,
        )
        .map_err(|error| error.to_string())?
    };

    Ok(NodeTrafficPanel {
        snapshot,
        session_upload: 0,
        session_download: 0,
        total_records: 0,
        history: Vec::new(),
    })
}

pub(crate) fn create_port_slot_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    input: PortSlotInput,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    mutate_and_persist(app, state, |settings_data| {
        settings::create_port_slot(settings_data, input).map(|_| ())
    })
}

pub(crate) fn update_port_slot_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    slot_id: String,
    input: PortSlotInput,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    mutate_and_persist(app, state, |settings_data| {
        settings::update_port_slot(settings_data, &slot_id, input)
    })
}

pub(crate) fn delete_port_slot_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    slot_id: String,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    mutate_and_persist(app, state, |settings_data| {
        if settings::delete_port_slot(settings_data, &slot_id) {
            Ok(())
        } else {
            Err("目标端口不存在".to_string())
        }
    })
}

pub(crate) fn set_port_slot_enabled_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    slot_id: String,
    enabled: bool,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    mutate_and_persist(app, state, |settings_data| {
        if settings::set_slot_enabled(settings_data, &slot_id, enabled) {
            Ok(())
        } else {
            Err("目标端口不存在".to_string())
        }
    })
}

pub(crate) fn bind_port_slot_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    slot_id: String,
    binding: PortSlotBindingInput,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    mutate_and_persist(app, state, |settings_data| {
        settings::bind_slot_node(settings_data, &slot_id, binding)
    })
}

pub(crate) fn bind_port_slots_batch_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    assignments: Vec<PortSlotBatchBindingInput>,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    if assignments.is_empty() {
        return Err("没有需要绑定的端口".to_string());
    }
    mutate_and_persist(app, state, |settings_data| {
        let mut seen = std::collections::BTreeSet::new();
        for assignment in assignments {
            if !seen.insert(assignment.slot_id.clone()) {
                return Err(format!("端口 {} 被重复提交", assignment.slot_id));
            }
            settings::bind_slot_node(
                settings_data,
                &assignment.slot_id,
                PortSlotBindingInput {
                    sub_id: assignment.sub_id,
                    node_index: assignment.node_index,
                },
            )?;
        }
        Ok(())
    })
}

pub(crate) fn clear_port_slot_binding_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    slot_id: String,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    mutate_and_persist(app, state, |settings_data| {
        if settings::clear_slot_binding(settings_data, &slot_id) {
            Ok(())
        } else {
            Err("目标端口不存在".to_string())
        }
    })
}

pub(crate) fn reorder_port_slots_impl<R: Runtime>(
    app: Option<&AppHandle<R>>,
    ordered_ids: Vec<String>,
    state: &AppState,
) -> Result<BootstrapPayload, String> {
    mutate_and_persist(app, state, |settings_data| {
        let previous = settings_data
            .port_slots
            .iter()
            .map(|slot| slot.id.clone())
            .collect::<Vec<_>>();
        settings::reorder_port_slots(settings_data, &ordered_ids);
        let next = settings_data
            .port_slots
            .iter()
            .map(|slot| slot.id.clone())
            .collect::<Vec<_>>();
        if next == ordered_ids || previous == ordered_ids {
            Ok(())
        } else {
            Err("端口排序数据无效".to_string())
        }
    })
}

pub(crate) fn port_traffic_impl(state: &AppState) -> Result<PortTrafficReport, String> {
    let settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    if !state.runner.is_running() {
        return Ok(state.port_traffic.sample(
            &settings_data,
            &serde_json::Value::Null,
            false,
        ));
    }
    let Some((controller_port, controller_secret)) = state.runner.controller_access() else {
        return Ok(state.port_traffic.sample(
            &settings_data,
            &serde_json::Value::Null,
            false,
        ));
    };
    let payload = traffic::fetch_port_traffic_payload(controller_port, &controller_secret)
        .map_err(|error| format!("连接数据获取失败，等待下一次采样：{error}"))?;
    Ok(state.port_traffic.sample(&settings_data, &payload, true))
}

pub(crate) fn validate_port_impl(
    port: u16,
    ignore_slot_id: Option<String>,
) -> Result<PortValidation, String> {
    let settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    Ok(settings::validate_local_port(
        &settings_data,
        port,
        ignore_slot_id.as_deref(),
    ))
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapPayload, String> {
    bootstrap_impl(&state)
}

#[tauri::command]
pub fn create_port_slot(
    app: AppHandle,
    input: PortSlotInput,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    create_port_slot_impl(Some(&app), input, &state)
}

#[tauri::command]
pub fn update_port_slot(
    app: AppHandle,
    slot_id: String,
    input: PortSlotInput,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    update_port_slot_impl(Some(&app), slot_id, input, &state)
}

#[tauri::command]
pub fn delete_port_slot(
    app: AppHandle,
    slot_id: String,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    delete_port_slot_impl(Some(&app), slot_id, &state)
}

#[tauri::command]
pub fn set_port_slot_enabled(
    app: AppHandle,
    slot_id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    set_port_slot_enabled_impl(Some(&app), slot_id, enabled, &state)
}

#[tauri::command]
pub fn bind_port_slot(
    app: AppHandle,
    slot_id: String,
    binding: PortSlotBindingInput,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    bind_port_slot_impl(Some(&app), slot_id, binding, &state)
}

#[tauri::command]
pub fn bind_port_slots_batch(
    app: AppHandle,
    assignments: Vec<PortSlotBatchBindingInput>,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    bind_port_slots_batch_impl(Some(&app), assignments, &state)
}

#[tauri::command]
pub fn clear_port_slot_binding(
    app: AppHandle,
    slot_id: String,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    clear_port_slot_binding_impl(Some(&app), slot_id, &state)
}

#[tauri::command]
pub fn reorder_port_slots(
    app: AppHandle,
    ordered_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    reorder_port_slots_impl(Some(&app), ordered_ids, &state)
}

#[tauri::command]
pub fn validate_port(port: u16, ignore_slot_id: Option<String>) -> Result<PortValidation, String> {
    validate_port_impl(port, ignore_slot_id)
}

#[tauri::command]
pub fn port_traffic(state: State<'_, AppState>) -> Result<PortTrafficReport, String> {
    port_traffic_impl(&state)
}

#[tauri::command]
pub async fn create_subscription(
    app: AppHandle,
    input: UpsertSubscriptionInput,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    create_subscription_with_import_impl(Some(&app), input, &state).await
}

#[tauri::command]
pub async fn update_subscription(
    app: AppHandle,
    sub_id: String,
    input: UpsertSubscriptionInput,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    update_subscription_with_import_impl(Some(&app), sub_id, input, &state).await
}

#[tauri::command]
pub fn delete_subscription(
    app: AppHandle,
    sub_id: String,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    delete_subscription_impl(Some(&app), sub_id, &state)
}

#[tauri::command]
pub fn delete_selected_nodes(
    app: AppHandle,
    sub_id: String,
    node_indices: Vec<usize>,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    delete_selected_nodes_impl(Some(&app), sub_id, node_indices, &state)
}

#[tauri::command]
pub fn reorder_subscriptions(
    ordered_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    reorder_subscriptions_impl(ordered_ids, &state)
}

#[tauri::command]
pub async fn import_subscription(
    app: AppHandle,
    sub_id: String,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    import_subscription_impl(Some(&app), sub_id, &state).await
}

#[tauri::command]
pub fn save_selection(
    app: AppHandle,
    sub_id: String,
    selected_node_indices: Vec<usize>,
    port_assignments: BTreeMap<String, u16>,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    save_selection_impl(
        Some(&app),
        sub_id,
        selected_node_indices,
        port_assignments,
        &state,
    )
}

#[tauri::command]
pub fn save_subconverter(
    url: String,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    save_subconverter_impl(url, &state)
}

#[tauri::command]
pub fn node_traffic_snapshot(
    sub_id: String,
    node_index: usize,
    state: State<'_, AppState>,
) -> Result<NodeTrafficSnapshot, String> {
    node_traffic_snapshot_impl(sub_id, node_index, &state)
}

#[tauri::command]
pub fn node_traffic_panel(
    sub_id: String,
    node_index: usize,
    state: State<'_, AppState>,
) -> Result<NodeTrafficPanel, String> {
    node_traffic_panel_impl(sub_id, node_index, &state)
}

#[tauri::command]
pub fn save_proxy_settings(
    enabled: bool,
    url: String,
    mihomo_path: String,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    save_proxy_settings_impl(enabled, url, mihomo_path, &state)
}

#[tauri::command]
pub fn save_node_remark(
    app: AppHandle,
    sub_id: String,
    node_index: usize,
    remark: String,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    save_node_remark_impl(Some(&app), sub_id, node_index, remark, &state)
}

#[tauri::command]
pub fn start_mihomo(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<BootstrapPayload, String> {
    start_mihomo_impl(Some(&app), &state)
}

#[tauri::command]
pub fn stop_mihomo(app: AppHandle, state: State<'_, AppState>) -> Result<BootstrapPayload, String> {
    stop_mihomo_impl(Some(&app), &state)
}

#[tauri::command]
pub fn cancel_latency(state: State<'_, AppState>) {
    cancel_latency_impl(&state);
}

#[tauri::command]
pub async fn test_latency(
    app: AppHandle,
    sub_id: String,
    node_indices: Vec<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<LatencyResult>, String> {
    let settings_data = settings::load_settings().map_err(|error| error.to_string())?;
    let Some(sub) = find_subscription(&settings_data, &sub_id) else {
        return Err("目标订阅不存在".to_string());
    };

    let nodes = node_indices
        .into_iter()
        .filter_map(|index| sub.nodes.get(index).cloned().map(|node| (index, node)))
        .collect::<Vec<_>>();

    if nodes.is_empty() {
        return Err("当前没有可测速的节点".to_string());
    }

    latency::test_nodes_latency(&app, &sub_id, &nodes, 5_000, &state.latency)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        env, fs,
        io::{Read, Write},
        net::TcpListener,
        path::{Path, PathBuf},
        sync::MutexGuard,
        thread,
    };

    use uuid::Uuid;

    use super::*;
    use crate::{
        models::{AppSettings, ProxyNode, SubscriptionRecord, UpsertSubscriptionInput},
        runtime_paths,
    };

    struct TestEnv {
        _guard: MutexGuard<'static, ()>,
        root: PathBuf,
    }

    impl TestEnv {
        fn new(name: &str) -> Self {
            let guard = runtime_paths::test_env_lock().lock().expect("env lock");
            let root = env::temp_dir().join(format!(
                "mihomo-switch-commands-{name}-{}-{}",
                std::process::id(),
                Uuid::new_v4()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("create runtime dir");
            env::set_var("MIHOMO_MANAGER_HOME", &root);
            env::set_var("MIHOMO_MANAGER_SKIP_LEGACY_IMPORT", "1");
            Self {
                _guard: guard,
                root,
            }
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            env::remove_var("MIHOMO_MANAGER_HOME");
            env::remove_var("MIHOMO_MANAGER_SKIP_LEGACY_IMPORT");
            let _ = fs::remove_dir_all(&self.root);
        }
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

    fn serve_subscription_once_after_request<F>(body: &str, after_request: F) -> String
    where
        F: FnOnce() + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local subscription server");
        let address = listener.local_addr().expect("subscription server address");
        let response_body = body.to_string();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 2048];
                let _ = stream.read(&mut buffer);
                after_request();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        format!("http://{address}/subscription")
    }

    fn serve_subscription_once(body: &str) -> String {
        serve_subscription_once_after_request(body, || {})
    }

    #[test]
    fn save_proxy_settings_rejects_invalid_enabled_proxy_without_persisting() {
        let _env = TestEnv::new("invalid-local-proxy");
        settings::save_settings(&AppSettings::default()).expect("seed settings");
        let state = AppState::default();

        let result = save_proxy_settings_impl(true, "not-a-url".to_string(), String::new(), &state);

        assert!(result.is_err());
        let saved = settings::load_settings().expect("reload settings");
        assert!(!saved.local_proxy_enabled);
        assert_eq!(
            saved.local_proxy_url,
            crate::models::default_local_proxy_url()
        );
    }

    #[test]
    fn create_subscription_parses_manual_content_immediately() {
        let _env = TestEnv::new("create-manual");
        let state = AppState::default();

        let payload = create_subscription_impl(
            None::<&AppHandle>,
            UpsertSubscriptionInput {
                name: "Manual Feed".to_string(),
                url: String::new(),
                manual: true,
                content: "vless://11111111-1111-1111-1111-111111111111@example.com:443?security=reality&pbk=test-key&sid=abcd#Manual%20Node".to_string(),
            },
            &state,
        )
        .expect("create manual subscription");

        let created = payload
            .settings
            .subscriptions
            .iter()
            .find(|item| item.name == "Manual Feed")
            .expect("created subscription");
        assert!(created.manual);
        assert_eq!(created.nodes.len(), 1);
        assert_eq!(created.nodes[0].name, "Manual Node");
        assert_eq!(created.nodes[0].node_type, "vless");

        let saved = settings::load_settings().expect("load settings");
        let persisted = saved
            .subscriptions
            .iter()
            .find(|item| item.id == created.id)
            .expect("persisted subscription");
        assert_eq!(persisted.nodes.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn create_url_subscription_fetches_nodes_before_persisting() {
        let _env = TestEnv::new("create-url-with-import");
        let remote_url = serve_subscription_once(
            "trojan://secret@example.com:443?sni=example.com#Created%20Remote",
        );
        let state = AppState::default();

        let payload = create_subscription_with_import_impl(
            None::<&AppHandle>,
            UpsertSubscriptionInput {
                name: "Remote Feed".to_string(),
                url: remote_url,
                manual: false,
                content: String::new(),
            },
            &state,
        )
        .await
        .expect("create and import URL subscription");

        let created = payload
            .settings
            .subscriptions
            .iter()
            .find(|item| item.name == "Remote Feed")
            .expect("created URL subscription");
        assert_eq!(created.nodes.len(), 1);
        assert_eq!(created.nodes[0].name, "Created Remote");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_url_subscription_update_preserves_existing_nodes_and_source() {
        let _env = TestEnv::new("update-url-failure-is-atomic");
        let old_node = proxy_node("Existing Node", "trojan", "old.example.com", 443);
        let mut initial = AppSettings::default();
        initial.subscriptions.push(subscription(
            "remote-sub",
            false,
            "",
            vec![old_node],
            Vec::new(),
            &[],
        ));
        settings::save_settings(&initial).expect("seed URL subscription");
        let invalid_url = serve_subscription_once("this is not a subscription");
        let state = AppState::default();

        let result = update_subscription_with_import_impl(
            None::<&AppHandle>,
            "remote-sub".to_string(),
            UpsertSubscriptionInput {
                name: "Changed Name".to_string(),
                url: invalid_url,
                manual: false,
                content: String::new(),
            },
            &state,
        )
        .await;

        assert!(result.is_err());
        let saved = settings::load_settings().expect("reload URL subscription");
        let preserved = find_subscription(&saved, "remote-sub").expect("preserved subscription");
        assert_eq!(preserved.name, "remote-sub");
        assert_eq!(preserved.url, "https://remote-sub.example.dev/sub");
        assert_eq!(preserved.nodes.len(), 1);
        assert_eq!(preserved.nodes[0].name, "Existing Node");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn changed_url_subscription_fetches_and_commits_new_nodes() {
        let _env = TestEnv::new("update-url-with-import");
        let old_node = proxy_node("Existing Node", "trojan", "old.example.com", 443);
        let mut initial = AppSettings::default();
        initial.subscriptions.push(subscription(
            "remote-sub",
            false,
            "",
            vec![old_node],
            Vec::new(),
            &[],
        ));
        settings::save_settings(&initial).expect("seed URL subscription");
        let remote_url = serve_subscription_once(
            "vless://11111111-1111-1111-1111-111111111111@new.example.com:443?security=reality&pbk=test-key&sid=abcd#New%20Remote",
        );
        let state = AppState::default();

        let payload = update_subscription_with_import_impl(
            None::<&AppHandle>,
            "remote-sub".to_string(),
            UpsertSubscriptionInput {
                name: "Updated Feed".to_string(),
                url: remote_url.clone(),
                manual: false,
                content: String::new(),
            },
            &state,
        )
        .await
        .expect("update and import URL subscription");

        let updated = find_subscription(&payload.settings, "remote-sub").expect("updated feed");
        assert_eq!(updated.name, "Updated Feed");
        assert_eq!(updated.url, remote_url);
        assert_eq!(updated.nodes.len(), 1);
        assert_eq!(updated.nodes[0].name, "New Remote");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn import_subscription_parses_manual_content_and_persists_nodes() {
        let _env = TestEnv::new("import-manual");
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
        })
        .expect("seed settings");
        let state = AppState::default();

        let payload =
            import_subscription_impl(None::<&AppHandle>, "manual-sub".to_string(), &state)
                .await
                .expect("import manual subscription");

        let imported = payload
            .settings
            .subscriptions
            .iter()
            .find(|item| item.id == "manual-sub")
            .expect("imported subscription");
        assert_eq!(imported.nodes.len(), 1);
        assert_eq!(imported.nodes[0].name, "Trojan Node");
        assert_eq!(imported.nodes[0].node_type, "trojan");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn import_subscription_fetches_remote_content_without_app_handle() {
        let _env = TestEnv::new("import-remote");
        let remote_url = serve_subscription_once(
            "trojan://secret@example.com:443?sni=example.com#Remote%20Trojan",
        );
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
        })
        .expect("seed remote subscription");
        let state = AppState::default();

        let payload =
            import_subscription_impl(None::<&AppHandle>, "remote-sub".to_string(), &state)
                .await
                .expect("import remote subscription");

        let imported = payload
            .settings
            .subscriptions
            .iter()
            .find(|item| item.id == "remote-sub")
            .expect("imported remote subscription");
        assert_eq!(imported.nodes.len(), 1);
        assert_eq!(imported.nodes[0].name, "Remote Trojan");
        assert_eq!(imported.nodes[0].node_type, "trojan");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn import_subscription_preserves_changes_made_while_fetching() {
        let _env = TestEnv::new("import-preserve-concurrent");
        let remote_url = serve_subscription_once_after_request(
            "trojan://secret@example.com:443?sni=example.com#Remote%20Trojan",
            || {
                let mut latest = settings::load_settings().expect("load concurrent settings");
                latest.mihomo_path = "changed-during-fetch.exe".to_string();
                settings::save_settings(&latest).expect("save concurrent settings");
            },
        );
        settings::save_settings(&AppSettings {
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
            ..AppSettings::default()
        })
        .expect("seed remote subscription");
        let state = AppState::default();

        let payload =
            import_subscription_impl(None::<&AppHandle>, "remote-sub".to_string(), &state)
                .await
                .expect("import remote subscription");

        assert_eq!(payload.settings.mihomo_path, "changed-during-fetch.exe");
        assert_eq!(payload.settings.subscriptions[0].nodes.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn import_subscription_discards_result_when_source_changes_while_fetching() {
        let _env = TestEnv::new("import-source-changed");
        let remote_url = serve_subscription_once_after_request(
            "trojan://secret@example.com:443?sni=example.com#Stale%20Node",
            || {
                let mut latest = settings::load_settings().expect("load concurrent settings");
                latest.subscriptions[0].url = "https://changed.example.dev/sub".to_string();
                settings::save_settings(&latest).expect("save concurrent settings");
            },
        );
        settings::save_settings(&AppSettings {
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
            ..AppSettings::default()
        })
        .expect("seed remote subscription");
        let state = AppState::default();

        let error = import_subscription_impl(None::<&AppHandle>, "remote-sub".to_string(), &state)
            .await
            .expect_err("stale result must be rejected");

        assert!(error.contains("订阅来源在更新期间已发生变化"));
        let saved = settings::load_settings().expect("reload settings");
        assert_eq!(
            saved.subscriptions[0].url,
            "https://changed.example.dev/sub"
        );
        assert!(saved.subscriptions[0].nodes.is_empty());
    }

    #[test]
    fn save_selection_reassigns_conflicting_ports_and_writes_config() {
        let _env = TestEnv::new("save-selection");
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
        })
        .expect("seed settings");
        let state = AppState::default();

        let mut requested_ports = BTreeMap::new();
        requested_ports.insert("0".to_string(), 10801);
        let payload = save_selection_impl(
            None::<&AppHandle>,
            "sub-b".to_string(),
            vec![0],
            requested_ports,
            &state,
        )
        .expect("save selection");

        let updated = payload
            .settings
            .subscriptions
            .iter()
            .find(|item| item.id == "sub-b")
            .expect("updated subscription");
        assert_eq!(updated.selected_node_indices, vec![0]);
        assert_eq!(updated.port_assignments.len(), 1);
        assert_ne!(updated.port_assignments.get("0"), Some(&10801));
        assert!(Path::new(&payload.runtime.config_path).exists());
    }

    #[test]
    fn update_subscription_reparses_manual_content_and_clears_selection() {
        let _env = TestEnv::new("update-manual");
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
        })
        .expect("seed settings");
        let state = AppState::default();

        let payload = update_subscription_impl(
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
        .expect("update manual subscription");

        let updated = payload
            .settings
            .subscriptions
            .iter()
            .find(|item| item.id == "manual-sub")
            .expect("updated manual subscription");
        assert_eq!(updated.name, "Manual Feed Updated");
        assert_eq!(updated.nodes.len(), 1);
        assert_eq!(updated.nodes[0].name, "Updated Node");
        assert_eq!(updated.nodes[0].node_type, "vless");
        assert!(updated.selected_node_indices.is_empty());
        assert!(updated.port_assignments.is_empty());
    }

    fn bound_slot(
        id: &str,
        port: u16,
        sub_id: &str,
        node: &crate::models::ProxyNode,
    ) -> crate::models::PortSlot {
        crate::models::PortSlot {
            id: id.to_string(),
            name: id.to_string(),
            note: String::new(),
            local_port: port,
            enabled: true,
            binding: Some(crate::models::NodeBinding {
                sub_id: sub_id.to_string(),
                fingerprint: settings::node_fingerprint(node),
                node_name: node.name.clone(),
            }),
        }
    }

    #[test]
    fn port_slot_ui_lifecycle_persists_and_updates_runtime_config() {
        let _env = TestEnv::new("port-slot-ui-lifecycle");
        let first = proxy_node("First Node", "vless", "first.example.com", 443);
        let second = proxy_node("Second Node", "trojan", "second.example.com", 443);
        let listener = TcpListener::bind("127.0.0.1:0").expect("reserve free port");
        let port = listener.local_addr().expect("free port address").port();
        drop(listener);
        settings::save_settings(&AppSettings {
            subscriptions: vec![subscription(
                "sub-a",
                false,
                "",
                vec![first, second],
                Vec::new(),
                &[],
            )],
            ..AppSettings::default()
        })
        .expect("seed subscription");
        let state = AppState::default();

        let created = create_port_slot_impl(
            None::<&AppHandle>,
            PortSlotInput {
                name: "UI Slot".to_string(),
                local_port: port,
                note: "browser profile".to_string(),
                enabled: true,
                binding: Some(PortSlotBindingInput {
                    sub_id: "sub-a".to_string(),
                    node_index: 0,
                }),
            },
            &state,
        )
        .expect("create port slot");
        let slot_id = created.settings.port_slots[0].id.clone();
        assert_eq!(created.slots[0].state, "valid");
        assert!(runtime_paths::pool_config_path().expect("config path").is_file());

        let conflict = validate_port_impl(port, None).expect("validate duplicate port");
        assert_eq!(conflict.status, "conflict");
        let unchanged = validate_port_impl(port, Some(slot_id.clone())).expect("validate same port");
        assert_eq!(unchanged.status, "ok");

        let disabled = set_port_slot_enabled_impl(
            None::<&AppHandle>,
            slot_id.clone(),
            false,
            &state,
        )
        .expect("disable slot");
        assert!(!disabled.settings.port_slots[0].enabled);
        assert!(!runtime_paths::pool_config_path().expect("config path").exists());

        let updated = update_port_slot_impl(
            None::<&AppHandle>,
            slot_id.clone(),
            PortSlotInput {
                name: "Updated Slot".to_string(),
                local_port: port,
                note: "updated note".to_string(),
                enabled: true,
                binding: Some(PortSlotBindingInput {
                    sub_id: "sub-a".to_string(),
                    node_index: 1,
                }),
            },
            &state,
        )
        .expect("update slot");
        assert_eq!(updated.slots[0].binding.as_ref().expect("binding").node_name, "Second Node");

        let cleared = clear_port_slot_binding_impl(
            None::<&AppHandle>,
            slot_id.clone(),
            &state,
        )
        .expect("clear binding");
        assert_eq!(cleared.slots[0].state, "unbound");

        let rebound = bind_port_slot_impl(
            None::<&AppHandle>,
            slot_id.clone(),
            PortSlotBindingInput {
                sub_id: "sub-a".to_string(),
                node_index: 0,
            },
            &state,
        )
        .expect("rebind slot");
        assert_eq!(rebound.slots[0].state, "valid");

        let deleted = delete_port_slot_impl(None::<&AppHandle>, slot_id, &state)
            .expect("delete slot");
        assert!(deleted.settings.port_slots.is_empty());
        assert!(!runtime_paths::pool_config_path().expect("config path").exists());
    }

    #[test]
    fn persist_runtime_settings_restores_files_when_config_generation_fails() {
        let _env = TestEnv::new("persist-runtime-rollback");
        let node = proxy_node("Stable Node", "vless", "stable.example.com", 443);
        let previous = AppSettings {
            subscriptions: vec![subscription(
                "sub-a",
                false,
                "",
                vec![node.clone()],
                Vec::new(),
                &[],
            )],
            port_slots: vec![bound_slot("slot-a", 10808, "sub-a", &node)],
            ..AppSettings::default()
        };
        settings::save_settings(&previous).expect("seed settings");
        config::write_pool_config(&previous).expect("seed config");
        let previous_config =
            std::fs::read_to_string(runtime_paths::pool_config_path().expect("config path"))
                .expect("read previous config");

        let mut invalid = previous.clone();
        invalid
            .port_slots
            .push(bound_slot("slot-b", 10808, "sub-a", &node));
        let state = AppState::default();

        let error = persist_runtime_settings(None::<&AppHandle>, &state, &invalid)
            .expect_err("duplicate port config must fail");

        assert!(error.to_string().contains("被多个已启用槽位重复使用"));
        let restored = settings::load_settings().expect("load restored settings");
        assert_eq!(restored.port_slots.len(), 1);
        assert_eq!(restored.port_slots[0].id, "slot-a");
        let restored_config =
            std::fs::read_to_string(runtime_paths::pool_config_path().expect("config path"))
                .expect("read restored config");
        assert_eq!(restored_config, previous_config);
    }

    #[test]
    fn batch_binding_is_atomic_when_any_assignment_is_invalid() {
        let _env = TestEnv::new("batch-binding-atomic");
        let original = proxy_node("Original", "vless", "original.example.com", 443);
        let replacement = proxy_node("Replacement", "vless", "replacement.example.com", 443);
        settings::save_settings(&AppSettings {
            subscriptions: vec![subscription(
                "sub-a",
                false,
                "",
                vec![original.clone(), replacement],
                Vec::new(),
                &[],
            )],
            port_slots: vec![bound_slot("slot-a", 10808, "sub-a", &original)],
            ..AppSettings::default()
        })
        .expect("seed settings");
        let state = AppState::default();

        let result = bind_port_slots_batch_impl(
            None::<&AppHandle>,
            vec![
                PortSlotBatchBindingInput {
                    slot_id: "slot-a".to_string(),
                    sub_id: "sub-a".to_string(),
                    node_index: 1,
                },
                PortSlotBatchBindingInput {
                    slot_id: "missing-slot".to_string(),
                    sub_id: "sub-a".to_string(),
                    node_index: 0,
                },
            ],
            &state,
        );

        assert!(result.is_err());
        let reloaded = settings::load_settings().expect("reload settings");
        assert_eq!(
            reloaded.port_slots[0]
                .binding
                .as_ref()
                .expect("original binding")
                .node_name,
            "Original"
        );
    }

    #[test]
    fn update_remote_subscription_source_clears_imported_nodes_and_config() {
        let _env = TestEnv::new("update-remote-source");
        let old_node = proxy_node("Old Remote Node", "trojan", "old.example.com", 443);
        settings::save_settings(&AppSettings {
            schema_version: 2,
            subscriptions: vec![subscription(
                "remote-sub",
                false,
                "",
                vec![old_node.clone()],
                vec![0],
                &[("0", 10801)],
            )],
            port_slots: vec![bound_slot("slot-a", 10801, "remote-sub", &old_node)],
            slots_migrated: true,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        })
        .expect("seed settings");
        let config_path =
            config::write_pool_config(&settings::load_settings().expect("load settings"))
                .expect("write initial config")
                .expect("initial config path");
        assert!(Path::new(&config_path).exists());
        let state = AppState::default();

        let payload = update_subscription_impl(
            None::<&AppHandle>,
            "remote-sub".to_string(),
            UpsertSubscriptionInput {
                name: "Remote Feed".to_string(),
                url: "https://new.example.dev/sub".to_string(),
                manual: false,
                content: String::new(),
            },
            &state,
        )
        .expect("update remote source");

        let updated = payload
            .settings
            .subscriptions
            .iter()
            .find(|item| item.id == "remote-sub")
            .expect("updated remote subscription");
        assert_eq!(updated.url, "https://new.example.dev/sub");
        assert!(updated.nodes.is_empty());
        assert!(updated.selected_node_indices.is_empty());
        assert!(updated.port_assignments.is_empty());
        assert!(!Path::new(&payload.runtime.config_path).exists());
    }

    #[test]
    fn update_remote_subscription_name_keeps_imported_nodes() {
        let _env = TestEnv::new("update-remote-name");
        settings::save_settings(&AppSettings {
            schema_version: 2,
            subscriptions: vec![subscription(
                "remote-sub",
                false,
                "",
                vec![proxy_node(
                    "Remote Node",
                    "trojan",
                    "remote.example.com",
                    443,
                )],
                vec![0],
                &[("0", 10801)],
            )],
            port_slots: Vec::new(),
            slots_migrated: false,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        })
        .expect("seed settings");
        let state = AppState::default();

        let payload = update_subscription_impl(
            None::<&AppHandle>,
            "remote-sub".to_string(),
            UpsertSubscriptionInput {
                name: "Renamed Feed".to_string(),
                url: "https://remote-sub.example.dev/sub".to_string(),
                manual: false,
                content: String::new(),
            },
            &state,
        )
        .expect("update remote name");

        let updated = payload
            .settings
            .subscriptions
            .iter()
            .find(|item| item.id == "remote-sub")
            .expect("updated remote subscription");
        assert_eq!(updated.name, "Renamed Feed");
        assert_eq!(updated.nodes.len(), 1);
        assert_eq!(updated.selected_node_indices, vec![0]);
        assert_eq!(updated.port_assignments.get("0"), Some(&10801));
    }

    #[test]
    fn delete_subscription_removes_runtime_config_when_last_selected_is_deleted() {
        let _env = TestEnv::new("delete-subscription");
        let node = proxy_node("A", "vmess", "a.example.com", 443);
        settings::save_settings(&AppSettings {
            schema_version: 2,
            subscriptions: vec![subscription(
                "sub-a",
                false,
                "",
                vec![node.clone()],
                vec![0],
                &[("0", 10801)],
            )],
            port_slots: vec![bound_slot("slot-a", 10801, "sub-a", &node)],
            slots_migrated: true,
            subconverter: String::new(),
            mihomo_path: crate::models::default_mihomo_path_string(),
            local_proxy_enabled: false,
            local_proxy_url: crate::models::default_local_proxy_url(),
        })
        .expect("seed settings");
        let config_path =
            config::write_pool_config(&settings::load_settings().expect("load settings"))
                .expect("write initial config")
                .expect("initial config path");
        assert!(Path::new(&config_path).exists());
        let state = AppState::default();

        let payload = delete_subscription_impl(None::<&AppHandle>, "sub-a".to_string(), &state)
            .expect("delete subscription");

        assert!(payload.settings.subscriptions.is_empty());
        assert!(!Path::new(&payload.runtime.config_path).exists());
    }

    #[test]
    fn reorder_subscriptions_persists_requested_order() {
        let _env = TestEnv::new("reorder-subscriptions");
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
        })
        .expect("seed settings");
        let state = AppState::default();

        let payload = reorder_subscriptions_impl(
            vec![
                "sub-c".to_string(),
                "sub-a".to_string(),
                "sub-b".to_string(),
            ],
            &state,
        )
        .expect("reorder subscriptions");

        let ordered_ids = payload
            .settings
            .subscriptions
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ordered_ids, vec!["sub-c", "sub-a", "sub-b"]);

        let reloaded = settings::load_settings().expect("reload settings");
        let persisted_ids = reloaded
            .subscriptions
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(persisted_ids, vec!["sub-c", "sub-a", "sub-b"]);
    }

    #[test]
    fn save_subconverter_persists_url() {
        let _env = TestEnv::new("save-subconverter");
        settings::save_settings(&AppSettings::default()).expect("seed empty settings");
        let state = AppState::default();

        let payload =
            save_subconverter_impl("https://subconverter.example.dev".to_string(), &state)
                .expect("save subconverter");

        assert_eq!(
            payload.settings.subconverter,
            "https://subconverter.example.dev"
        );
        assert_eq!(
            settings::load_settings()
                .expect("reload settings")
                .subconverter,
            "https://subconverter.example.dev"
        );
    }

    #[test]
    fn start_mihomo_requires_at_least_one_selected_node() {
        let _env = TestEnv::new("start-empty");
        settings::save_settings(&AppSettings::default()).expect("seed empty settings");
        let state = AppState::default();

        let result = start_mihomo_impl(None::<&AppHandle>, &state);
        assert_eq!(
            result.unwrap_err(),
            "请先创建并启用至少一个绑定有效节点的端口"
        );
    }
}
