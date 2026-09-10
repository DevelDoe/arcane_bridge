mod activity_log;
mod bridge_admin;
mod console_window;
mod hub;
mod hub_runtime;
mod shell_visibility;
mod updates;
mod zone_mode;

use bridge_admin::BridgeStatus;
use console_window::{emit_console_update, open_console, ConsoleState, SharedConsoleState};
use hub_runtime::{
    acquire_singleton_lock, bridge_host_from_env, bridge_port_from_env, start_in_process_hub,
};
use shell_visibility::apply_tray_only_shell;

use hub_runtime::HubEvent;
pub use shell_visibility::prepare_windows_tray_process;
use std::sync::{mpsc::Receiver, Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent};
use zone_mode::ZoneModeState;

const TRAY_ID: &str = "arcane-bridge-tray";

fn title_version_line(app: &AppHandle) -> String {
    let pkg = app.package_info();
    format!("{}  v{}", pkg.name, pkg.version)
}

fn build_tray_menu(
    app: &AppHandle,
    status: &BridgeStatus,
) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let title = MenuItem::with_id(app, "title", title_version_line(app), false, None::<&str>)?;
    let console = MenuItem::with_id(app, "console", "Bridge Console…", true, None::<&str>)?;
    let check_updates = MenuItem::with_id(
        app,
        "check_updates",
        "Check for updates…",
        true,
        None::<&str>,
    )?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Arcane Bridge", true, None::<&str>)?;

    let mut connected: Vec<MenuItem<tauri::Wry>> = Vec::new();
    if status.apps.monitor || status.apps.guilds {
        connected.push(MenuItem::with_id(
            app,
            "zoning",
            "Arcane Zoning",
            true,
            None::<&str>,
        )?);
    }
    if status.apps.caster {
        connected.push(MenuItem::with_id(
            app,
            "caster",
            "Caster",
            false,
            None::<&str>,
        )?);
    }
    if status.apps.guilds {
        connected.push(MenuItem::with_id(
            app,
            "guilds",
            "Guilds",
            false,
            None::<&str>,
        )?);
    }

    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&title];
    for item in &connected {
        items.push(item);
    }
    if !connected.is_empty() {
        items.push(&sep1);
    }
    items.push(&console);
    items.push(&sep2);
    items.push(&check_updates);
    items.push(&sep3);
    items.push(&quit);

    Menu::with_items(app, &items)
}

fn refresh_tray_menu(app: &AppHandle, status: &BridgeStatus) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = build_tray_menu(app, status) {
        let _ = tray.set_menu(Some(menu));
    }
    let mut tip_parts: Vec<&str> = Vec::new();
    if status.apps.monitor {
        tip_parts.push("Monitor");
    }
    if status.apps.caster {
        tip_parts.push("Caster");
    }
    if status.apps.guilds {
        tip_parts.push("Guilds");
    }
    let tip = if status.listening {
        if tip_parts.is_empty() {
            "Arcane Bridge — hub up, waiting for apps".to_string()
        } else {
            format!("Arcane Bridge — {}", tip_parts.join(", "))
        }
    } else {
        "Arcane Bridge — reconnecting…".to_string()
    };
    let _ = tray.set_tooltip(Some(tip));
}

fn spawn_status_listener(app: AppHandle, rx: Receiver<BridgeStatus>, state: SharedConsoleState) {
    std::thread::spawn(move || {
        while let Ok(status) = rx.recv() {
            let handle = app.clone();
            let state = state.clone();
            let _ = app.run_on_main_thread(move || {
                if let Ok(mut guard) = state.lock() {
                    guard.apply_status(status.clone());
                }
                refresh_tray_menu(&handle, &status);
                emit_console_update(&handle, &state);
            });
        }
    });
}

fn spawn_hub_event_listener(app: AppHandle, rx: Receiver<HubEvent>) {
    std::thread::spawn(move || {
        let mut last_hydration: Option<std::time::Instant> = None;
        while let Ok(event) = rx.recv() {
            if matches!(
                event,
                HubEvent::MonitorConnected | HubEvent::MonitorZonesRequested
            ) {
                let now = std::time::Instant::now();
                if last_hydration.is_some_and(|previous| {
                    now.duration_since(previous) < std::time::Duration::from_millis(500)
                }) {
                    continue;
                }
                last_hydration = Some(now);
            }
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || match event {
                HubEvent::MonitorConnected | HubEvent::MonitorZonesRequested => {
                    if let Err(error) = zone_mode::hydrate_monitor_views(&handle) {
                        eprintln!("[arcane-bridge] hydrate Monitor zoning: {error}");
                    }
                }
                HubEvent::MonitorViewUnzoned { view_id, cause } => {
                    if let Err(error) = zone_mode::unassign_monitor_view(&handle, &view_id, &cause)
                    {
                        eprintln!("[arcane-bridge] unassign Monitor view: {error}");
                    }
                }
                HubEvent::MonitorTraderPoolUnzoned { pool } => {
                    if let Err(error) = zone_mode::unassign_monitor_trader_pool(&handle, &pool) {
                        eprintln!("[arcane-bridge] unassign Monitor trader pool: {error}");
                    }
                }
                HubEvent::MonitorTraderPoolsPublished => {
                    if let Err(error) = zone_mode::refresh_assigned_view_ids(&handle) {
                        eprintln!("[arcane-bridge] refresh Zone Mode assignments: {error}");
                    }
                }
                HubEvent::GuildsConnected => {
                    if let Err(error) = zone_mode::hydrate_guilds_views(&handle) {
                        eprintln!("[arcane-bridge] hydrate Guilds zoning: {error}");
                    }
                }
            });
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _singleton = match acquire_singleton_lock() {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("[arcane-bridge] {e}");
            std::process::exit(0);
        }
    };

    let app = tauri::Builder::default()
        .manage(ZoneModeState::default())
        .invoke_handler(tauri::generate_handler![
            zone_mode::zone_mode_initial,
            zone_mode::zone_mode_update,
            zone_mode::zone_mode_place,
            zone_mode::zone_mode_close_view,
            zone_mode::zone_mode_cancel,
            zone_mode::zone_mode_finish
        ])
        .setup(|app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            #[cfg(target_os = "macos")]
            app.set_dock_visibility(false);

            let bridge_version = app.package_info().version.to_string();
            let (hub_rx, hub_event_rx, hub_control) = start_in_process_hub(&bridge_version)
                .map_err(|e| {
                    eprintln!("[arcane-bridge] hub start: {e}");
                    std::io::Error::other(e)
                })?;
            app.manage(hub_control);

            let initial = BridgeStatus {
                listening: true,
                host: bridge_host_from_env(),
                port: bridge_port_from_env(),
                ..Default::default()
            };

            let version = app.package_info().version.to_string();
            let console_state: SharedConsoleState =
                Arc::new(Mutex::new(ConsoleState::new(version, initial.clone())));
            app.manage(console_state.clone());

            spawn_status_listener(app.handle().clone(), hub_rx, console_state.clone());
            spawn_hub_event_listener(app.handle().clone(), hub_event_rx);

            let menu = build_tray_menu(app.handle(), &initial)?;
            let icon = app.default_window_icon().cloned().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "missing tray icon")
            })?;

            let tray_app = app.handle().clone();
            let tray_state = console_state.clone();
            let _tray = TrayIconBuilder::with_id(TRAY_ID)
                .icon(icon)
                .menu(&menu)
                .tooltip("Arcane Bridge")
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "console" => {
                        let state = tray_state.clone();
                        let app_for_thread = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            if let Err(e) = open_console(&app_for_thread, &state) {
                                eprintln!("[arcane-bridge] console: {e}");
                            }
                        });
                    }
                    "check_updates" => {
                        let app_for_update = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = updates::check_and_install(&app_for_update).await {
                                eprintln!("[arcane-bridge] update: {e}");
                            }
                        });
                    }
                    "zoning" => {
                        if let Err(e) = zone_mode::start_zone_mode(app) {
                            eprintln!("[arcane-bridge] Zone Mode: {e}");
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            refresh_tray_menu(&tray_app, &initial);
            apply_tray_only_shell(app.handle());

            #[cfg(desktop)]
            updates::spawn_auto_updater(app.handle().clone());

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building arcane bridge tray app");

    app.run(|app_handle, event| {
        // Bridge is a tray application. Closing its last temporary window
        // (for example the Zone Mode overlays) must not terminate the hub.
        // Explicit app.exit(...) requests include an exit code and still pass.
        if let RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
        apply_tray_only_shell(app_handle);
    });
}
