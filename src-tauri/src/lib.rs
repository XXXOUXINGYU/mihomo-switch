mod commands;
mod config;
mod latency;
mod logging;
mod models;
mod parser;
mod runner;
mod runtime_paths;
mod settings;
mod state;
mod traffic;

#[cfg(not(test))]
use crate::state::AppState;
#[cfg(not(test))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(test))]
use tauri::{
    menu::{MenuBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime, WindowEvent,
};

#[cfg(not(test))]
const MAIN_WINDOW_LABEL: &str = "main";
#[cfg(not(test))]
const TRAY_SHOW_ID: &str = "tray_show";
#[cfg(not(test))]
const TRAY_QUIT_ID: &str = "tray_quit";

#[cfg(not(test))]
#[derive(Default)]
struct ExitGuard {
    quitting: AtomicBool,
}

#[cfg(not(test))]
fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(not(test))]
fn quit_application<R: Runtime>(app: &AppHandle<R>) {
    app.state::<ExitGuard>()
        .quitting
        .store(true, Ordering::SeqCst);

    if let Err(error) = app.state::<AppState>().runner.stop(app) {
        log::warn!("failed to stop mihomo before exit: {error}");
    }

    app.exit(0);
}

#[cfg(not(test))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .manage(ExitGuard::default())
        .setup(|app| {
            match runtime_paths::install_bundled_mihomo(app.handle()) {
                Ok(Some(path)) => log::info!("installed bundled mihomo: {}", path.display()),
                Ok(None) => {}
                Err(error) => log::warn!("failed to install bundled mihomo: {error}"),
            }
            app.state::<AppState>().traffic.start(app.handle());

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let show_item = tauri::menu::MenuItem::with_id(
                app,
                TRAY_SHOW_ID,
                "显示主窗口",
                true,
                None::<&str>,
            )?;
            let quit_item =
                tauri::menu::MenuItem::with_id(app, TRAY_QUIT_ID, "退出", true, None::<&str>)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .item(&PredefinedMenuItem::separator(app)?)
                .item(&quit_item)
                .build()?;

            let tray_icon = app
                .default_window_icon()
                .cloned()
                .expect("missing default window icon");

            TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .tooltip("Mihomo Switch")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id() == TRAY_SHOW_ID {
                        show_main_window(app);
                    } else if event.id() == TRAY_QUIT_ID {
                        quit_application(app);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if !window.state::<ExitGuard>().quitting.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::create_subscription,
            commands::update_subscription,
            commands::delete_subscription,
            commands::delete_selected_nodes,
            commands::reorder_subscriptions,
            commands::import_subscription,
            commands::save_selection,
            commands::save_node_remark,
            commands::create_port_slot,
            commands::update_port_slot,
            commands::delete_port_slot,
            commands::set_port_slot_enabled,
            commands::bind_port_slot,
            commands::bind_port_slots_batch,
            commands::clear_port_slot_binding,
            commands::reorder_port_slots,
            commands::validate_port,
            commands::port_traffic,
            commands::save_subconverter,
            commands::save_proxy_settings,
            commands::start_mihomo,
            commands::stop_mihomo,
            commands::cancel_latency,
            commands::test_latency,
            commands::node_traffic_snapshot,
            commands::node_traffic_panel
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mihomo Switch");
}

#[cfg(test)]
pub fn run() {}
