//! Platform-wide zoning overlays, one frameless editor per connected display.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewUrl,
    WebviewWindowBuilder,
};

use crate::hub_runtime::HubControl;

const ZONE_LABEL_PREFIX: &str = "zone-overlay-";
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);
static RECENT_PLACEMENTS: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneRect {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_app_id")]
    pub app_id: String,
    #[serde(default)]
    pub view_id: Option<String>,
    #[serde(default)]
    pub view_label: Option<String>,
    #[serde(default)]
    pub trader_pool: Option<String>,
}

fn default_app_id() -> String {
    "monitor".to_string()
}

fn assigned_zone_ids(sessions: &HashMap<String, DisplayZones>, monitor_claimed_active: bool) -> Vec<String> {
    let mut ids: Vec<String> = sessions
        .values()
        .flat_map(|session| session.zones.iter())
        .filter_map(|zone| zone.view_id.clone())
        .collect();
    let bridge_has_active = sessions
        .values()
        .flat_map(|session| session.zones.iter())
        .any(|zone| zone.trader_pool.as_deref() == Some("active"));
    if monitor_claimed_active || bridge_has_active {
        ids.push("__trader_active".to_string());
    }
    ids
}

fn take_exclusive_active(sessions: &mut HashMap<String, DisplayZones>, current_label: &str) -> bool {
    let current_has_active = sessions
        .get(current_label)
        .map(|session| {
            session
                .zones
                .iter()
                .any(|zone| zone.trader_pool.as_deref() == Some("active"))
        })
        .unwrap_or(false);
    if !current_has_active {
        return false;
    }
    for (label, session) in sessions.iter_mut() {
        let mut seen = false;
        for zone in &mut session.zones {
            if zone.trader_pool.as_deref() != Some("active") {
                continue;
            }
            if label != current_label || seen {
                zone.trader_pool = None;
            } else {
                seen = true;
            }
        }
    }
    true
}

fn clear_trader_pool(sessions: &mut HashMap<String, DisplayZones>, pool: &str) -> bool {
    let mut changed = false;
    for session in sessions.values_mut() {
        for zone in &mut session.zones {
            if zone.trader_pool.as_deref() == Some(pool) {
                zone.trader_pool = None;
                changed = true;
            }
        }
    }
    changed
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorView {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneApp {
    pub id: String,
    pub label: String,
    pub views: Vec<MonitorView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneModeInitial {
    pub zones: Vec<ZoneRect>,
    pub apps: Vec<ZoneApp>,
    pub assigned_view_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedZone {
    pub display_id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
struct DisplayZones {
    display_id: String,
    persistence_key: String,
    origin_x: f64,
    origin_y: f64,
    zones: Vec<ZoneRect>,
}

#[derive(Default)]
pub struct ZoneModeState(Mutex<HashMap<String, DisplayZones>>);

type SavedLayouts = HashMap<String, Vec<ZoneRect>>;

fn monitor_layout_key(monitor: &tauri::Monitor) -> String {
    let position = monitor.position();
    let size = monitor.size();
    let scale = monitor.scale_factor();
    format!(
        "{}|{}:{}|{}x{}|{scale}",
        monitor.name().map(String::as_str).unwrap_or("unnamed"),
        position.x,
        position.y,
        size.width,
        size.height
    )
}

fn trader_pool_payload(sessions: &HashMap<String, DisplayZones>) -> serde_json::Value {
    let mut active = Vec::new();
    let mut watch = Vec::new();
    for session in sessions.values() {
        for zone in &session.zones {
            if zone.app_id != "monitor" {
                continue;
            }
            let target = match zone.trader_pool.as_deref() {
                Some("active") => &mut active,
                Some("watch") => &mut watch,
                _ => continue,
            };
            target.push(serde_json::json!({
                "zoneId": format!("{}:{}", session.persistence_key, zone.id),
                "bounds": {
                    "x": (session.origin_x + zone.x).round(),
                    "y": (session.origin_y + zone.y).round(),
                    "width": zone.width.round().max(1.0),
                    "height": zone.height.round().max(1.0),
                }
            }));
        }
    }
    serde_json::json!({ "active": active, "watch": watch })
}

fn layouts_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("zone-layouts.json"))
        .map_err(|e| e.to_string())
}

fn load_layouts(app: &AppHandle) -> SavedLayouts {
    let Ok(path) = layouts_path(app) else {
        return HashMap::new();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str(&contents).unwrap_or_else(|e| {
        eprintln!("[arcane-bridge] read saved Zone Mode layouts: {e}");
        HashMap::new()
    })
}

fn save_layouts(app: &AppHandle, sessions: &HashMap<String, DisplayZones>) -> Result<(), String> {
    let path = layouts_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Preserve layouts for displays that are not connected during this session.
    let mut saved = load_layouts(app);
    for session in sessions.values() {
        saved.insert(session.persistence_key.clone(), session.zones.clone());
    }
    let json = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn unassign_view(
    app: &AppHandle,
    app_id: &str,
    view_id: &str,
    cause: &str,
) -> Result<(), String> {
    if matches!(cause, "closed" | "docked" | "moved" | "resized") {
        let recently_placed = RECENT_PLACEMENTS
            .lock()
            .ok()
            .and_then(|placements| placements.get(view_id).copied())
            .is_some_and(|placed_at| placed_at.elapsed() < Duration::from_secs(2));
        if recently_placed {
            eprintln!(
                "[arcane-bridge] ignored transient {cause} while placing {app_id}/{view_id}"
            );
            return Ok(());
        }
    }
    let mut changed = false;
    let state = app.state::<ZoneModeState>();
    if let Ok(mut sessions) = state.0.lock() {
        for session in sessions.values_mut() {
            for zone in &mut session.zones {
                if zone.app_id == app_id && zone.view_id.as_deref() == Some(view_id) {
                    zone.view_id = None;
                    zone.view_label = None;
                    changed = true;
                }
            }
        }
    }

    let mut saved = load_layouts(app);
    for zones in saved.values_mut() {
        for zone in zones {
            if zone.app_id == app_id && zone.view_id.as_deref() == Some(view_id) {
                zone.view_id = None;
                zone.view_label = None;
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(());
    }
    let path = layouts_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    app.emit(
        "zone-mode-view-unzoned",
        serde_json::json!({ "appId": app_id, "viewId": view_id, "cause": cause }),
    )
    .map_err(|e| e.to_string())
}

pub fn refresh_assigned_view_ids(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ZoneModeState>();
    let hub = app.state::<HubControl>();
    let sessions = state.0.lock().map_err(|e| e.to_string())?;
    let assigned_view_ids = assigned_zone_ids(&sessions, hub.monitor_active_pool());
    app.emit(
        "zone-mode-assignments-updated",
        serde_json::json!({ "assignedViewIds": assigned_view_ids }),
    )
    .map_err(|e| e.to_string())
}

pub fn unassign_monitor_trader_pool(app: &AppHandle, pool: &str) -> Result<(), String> {
    if pool != "active" {
        return Ok(());
    }
    let state = app.state::<ZoneModeState>();
    let hub = app.state::<HubControl>();
    if let Ok(mut sessions) = state.0.lock() {
        clear_trader_pool(&mut sessions, pool);
        let _ = hub.configure_monitor_trader_pools("zone-pools-unassign", trader_pool_payload(&sessions));
        let assigned_view_ids = assigned_zone_ids(&sessions, true);
        app.emit(
            "zone-mode-assignments-updated",
            serde_json::json!({ "assignedViewIds": assigned_view_ids }),
        )
        .map_err(|e| e.to_string())?;
    }

    let mut saved = load_layouts(app);
    for zones in saved.values_mut() {
        for zone in zones {
            if zone.trader_pool.as_deref() == Some(pool) {
                zone.trader_pool = None;
            }
        }
    }
    let path = layouts_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    app.emit(
        "zone-mode-trader-pool-unzoned",
        serde_json::json!({ "pool": pool }),
    )
    .map_err(|e| e.to_string())
}

pub fn hydrate_monitor_views(app: &AppHandle) -> Result<(), String> {
    let saved = load_layouts(app);
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let mut assigned = std::collections::HashSet::new();
    let mut placements = Vec::new();

    for monitor in monitors {
        let key = monitor_layout_key(&monitor);
        let Some(zones) = saved.get(&key) else {
            continue;
        };
        let scale = monitor.scale_factor();
        let position = monitor.position();
        let origin_x = position.x as f64 / scale;
        let origin_y = position.y as f64 / scale;
        for zone in zones {
            if zone.app_id != "monitor" {
                continue;
            }
            let Some(view_id) = zone.view_id.as_deref() else {
                continue;
            };
            if !assigned.insert(view_id.to_string()) {
                continue;
            }
            placements.push(serde_json::json!({
                "viewId": view_id,
                "bounds": {
                    "x": (origin_x + zone.x).round(),
                    "y": (origin_y + zone.y).round(),
                    "width": zone.width.round().max(1.0),
                    "height": zone.height.round().max(1.0),
                }
            }));
        }
    }

    let request_id = format!(
        "zone-hydrate-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default()
    );
    let count = placements.len();
    let hub = app.state::<HubControl>();
    hub.hydrate_monitor_zones(&request_id, serde_json::Value::Array(placements))?;
    // Build the same pool snapshot from the saved layouts for boot hydration.
    let mut pool_sessions = HashMap::new();
    for monitor in app.available_monitors().map_err(|e| e.to_string())? {
        let key = monitor_layout_key(&monitor);
        let Some(zones) = saved.get(&key).cloned() else {
            continue;
        };
        let scale = monitor.scale_factor();
        let position = monitor.position();
        pool_sessions.insert(
            key.clone(),
            DisplayZones {
                display_id: key.clone(),
                persistence_key: key,
                origin_x: position.x as f64 / scale,
                origin_y: position.y as f64 / scale,
                zones,
            },
        );
    }
    hub.configure_monitor_trader_pools(&request_id, trader_pool_payload(&pool_sessions))?;
    eprintln!("[arcane-bridge] hydrated {count} Monitor zone assignment(s)");
    Ok(())
}

pub fn hydrate_guilds_views(app: &AppHandle) -> Result<(), String> {
    let saved = load_layouts(app);
    let hub = app.state::<HubControl>();
    let mut count = 0usize;
    for monitor in app.available_monitors().map_err(|e| e.to_string())? {
        let key = monitor_layout_key(&monitor);
        let Some(zones) = saved.get(&key) else {
            continue;
        };
        let scale = monitor.scale_factor();
        let position = monitor.position();
        let origin_x = position.x as f64 / scale;
        let origin_y = position.y as f64 / scale;
        for zone in zones {
            if zone.app_id != "guilds" {
                continue;
            }
            let Some(view_id) = zone.view_id.as_deref() else {
                continue;
            };
            if view_id == "sidebar" {
                continue;
            }
            let request_id = format!("guilds-zone-hydrate-{view_id}-{count}");
            hub.place_guilds_view(
                &request_id,
                serde_json::json!({
                    "viewId": view_id,
                    "bounds": {
                        "x": (origin_x + zone.x).round(),
                        "y": (origin_y + zone.y).round(),
                        "width": zone.width.round().max(1.0),
                        "height": zone.height.round().max(1.0),
                    }
                }),
            )?;
            count += 1;
        }
    }
    eprintln!("[arcane-bridge] hydrated {count} Guilds zone assignment(s)");
    Ok(())
}

fn close_overlays(app: &AppHandle) {
    let overlays: Vec<_> = app
        .webview_windows()
        .into_values()
        .filter(|window| window.label().starts_with(ZONE_LABEL_PREFIX))
        .collect();
    if overlays.is_empty() {
        return;
    }

    // wry 0.55 nulls the WebView2 controller inside WM_DESTROY, then still
    // handles WM_SETFOCUS / WM_SIZE by dereferencing it. Closing the focused
    // overlay delivers those messages and aborts the process.
    for window in &overlays {
        disarm_webview_teardown(window);
    }
    for window in &overlays {
        let _ = window.hide();
    }
    release_keyboard_focus();
    eprintln!(
        "[arcane-bridge] Zone Mode: closing {} overlay(s)",
        overlays.len()
    );
    for window in overlays {
        let _ = window.close();
    }
}

#[cfg(windows)]
fn disarm_webview_teardown(window: &tauri::WebviewWindow) {
    use windows::Win32::UI::Shell::SetWindowSubclass;

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let installed = unsafe {
        SetWindowSubclass(
            hwnd,
            Some(zone_close_guard_proc),
            ZONE_CLOSE_GUARD_SUBCLASS_ID,
            0,
        )
    };
    if !installed.as_bool() {
        eprintln!("[arcane-bridge] Zone Mode: failed to guard WebView2 teardown");
    }
}

#[cfg(not(windows))]
fn disarm_webview_teardown(_window: &tauri::WebviewWindow) {}

#[cfg(windows)]
fn release_keyboard_focus() {
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    unsafe {
        let _ = SetFocus(None);
    }
}

#[cfg(not(windows))]
fn release_keyboard_focus() {}

#[cfg(windows)]
const ZONE_CLOSE_GUARD_SUBCLASS_ID: usize = 0xAB21;

#[cfg(windows)]
unsafe extern "system" fn zone_close_guard_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _uidsubclass: usize,
    _dwrefdata: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::Shell::DefSubclassProc;
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_ENTERSIZEMOVE, WM_MOVE, WM_MOVING, WM_SETFOCUS, WM_SIZE,
    };

    match msg {
        WM_SETFOCUS | WM_ENTERSIZEMOVE | WM_SIZE | WM_MOVE | WM_MOVING => LRESULT(0),
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

/// WebView2 panics if the overlay is destroyed while still inside its own invoke.
fn close_overlays_after_ipc(app: &AppHandle) {
    let generation = OVERLAY_GENERATION.load(Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        let closer = app.clone();
        let _ = app.run_on_main_thread(move || {
            if OVERLAY_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            close_overlays(&closer);
        });
    });
}

fn publish_zone_apps(app: &AppHandle) {
    let apps = zone_apps(&app.state::<HubControl>());
    let _ = app.emit("zone-mode-apps-updated", apps);
}

pub fn start_zone_mode(app: &AppHandle) -> Result<(), String> {
    OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst);
    close_overlays(app);

    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    eprintln!(
        "[arcane-bridge] Zone Mode: {} monitor(s) from available_monitors()",
        monitors.len()
    );
    for (index, monitor) in monitors.iter().enumerate() {
        let position = monitor.position();
        let size = monitor.size();
        let work_area = monitor.work_area();
        eprintln!(
            "[arcane-bridge] Zone Mode monitor[{index}]: name={:?} pos={}x{} size={}x{} scale={} work_pos={}x{} work_size={}x{}",
            monitor.name(),
            position.x,
            position.y,
            size.width,
            size.height,
            monitor.scale_factor(),
            work_area.position.x,
            work_area.position.y,
            work_area.size.width,
            work_area.size.height
        );
    }
    if monitors.is_empty() {
        return Err("no displays available for Zone Mode".into());
    }

    let saved_layouts = load_layouts(app);
    let state = app.state::<ZoneModeState>();
    let mut sessions = HashMap::new();
    let mut overlays = Vec::with_capacity(monitors.len());

    for (index, monitor) in monitors.into_iter().enumerate() {
        let label = format!("{ZONE_LABEL_PREFIX}{index}");
        let display_id = format!("display-{index}");
        let scale = monitor.scale_factor();
        let position = monitor.position();
        let size = monitor.size();
        let origin_x = position.x as f64 / scale;
        let origin_y = position.y as f64 / scale;
        let width = size.width as f64 / scale;
        let height = size.height as f64 / scale;
        let work_area = monitor.work_area();
        let work_left = work_area.position.x as f64 / scale - origin_x;
        let work_top = work_area.position.y as f64 / scale - origin_y;
        let work_width = work_area.size.width as f64 / scale;
        let work_height = work_area.size.height as f64 / scale;
        let persistence_key = monitor_layout_key(&monitor);
        let zones = saved_layouts
            .get(&persistence_key)
            .cloned()
            .unwrap_or_default();

        sessions.insert(
            label.clone(),
            DisplayZones {
                display_id: display_id.clone(),
                persistence_key,
                origin_x,
                origin_y,
                zones,
            },
        );

        let url = format!(
            "zone.html?display={display_id}&x={origin_x}&y={origin_y}&width={width}&height={height}&workLeft={work_left}&workTop={work_top}&workWidth={work_width}&workHeight={work_height}"
        );
        overlays.push((index, label, display_id, url, *position, *size));
    }

    // WebView2 creation pumps Windows messages, including IPC from earlier
    // overlays. Publish every session and release the mutex before building
    // any window so zone_mode_initial cannot deadlock on this thread.
    *state.0.lock().map_err(|e| e.to_string())? = sessions;

    for (index, label, display_id, url, position, size) in overlays {
        eprintln!(
            "[arcane-bridge] Zone Mode: creating overlay {label} for {display_id} at physical {}x{} {}x{}",
            position.x, position.y, size.width, size.height
        );
        // Initialize hidden at the primary origin, then apply the target display's
        // physical bounds before showing the transparent overlay.
        let overlay = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
            .title("Arcane Bridge Zone Mode")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .position(0.0, 0.0)
            .inner_size(800.0, 600.0)
            .focused(false)
            .build()
            .map_err(|e| format!("create Zone Mode overlay for {display_id}: {e}"))?;
        overlay
            .set_position(Position::Physical(PhysicalPosition::new(
                position.x,
                position.y,
            )))
            .map_err(|e| format!("position Zone Mode overlay for {display_id}: {e}"))?;
        overlay
            .set_size(Size::Physical(PhysicalSize::new(size.width, size.height)))
            .map_err(|e| format!("size Zone Mode overlay for {display_id}: {e}"))?;
        overlay
            .show()
            .map_err(|e| format!("show Zone Mode overlay for {display_id}: {e}"))?;
        if index == 0 {
            let _ = overlay.set_focus();
        }
        eprintln!("[arcane-bridge] Zone Mode: overlay {label} ready");
    }

    publish_zone_apps(app);
    Ok(())
}

pub fn zone_apps(hub: &HubControl) -> Vec<ZoneApp> {
    let monitor_views = hub
        .monitor_views()
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect();
    let mut apps = Vec::new();
    if hub.monitor_connected() {
        apps.push(ZoneApp {
            id: "monitor".into(),
            label: "Arcane Monitor".into(),
            views: monitor_views,
        });
    }
    if hub.guilds_connected() {
        apps.push(ZoneApp {
            id: "guilds".into(),
            label: "Arcane Guilds".into(),
            views: vec![
                MonitorView {
                    id: "chat".into(),
                    label: "Chat".into(),
                },
                MonitorView {
                    id: "header".into(),
                    label: "Header".into(),
                },
                MonitorView {
                    id: "toplist".into(),
                    label: "Focus & Watchlist".into(),
                },
            ],
        });
    }
    apps
}

#[tauri::command]
pub fn zone_mode_initial(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, ZoneModeState>,
    hub: tauri::State<'_, HubControl>,
) -> Result<ZoneModeInitial, String> {
    let sessions = state.0.lock().map_err(|e| e.to_string())?;
    let zones = sessions
        .get(window.label())
        .map(|session| session.zones.clone())
        .ok_or_else(|| "unknown Zone Mode overlay".to_string())?;
    let apps = zone_apps(&hub);
    let assigned_view_ids = assigned_zone_ids(&sessions, hub.monitor_active_pool());
    Ok(ZoneModeInitial {
        zones,
        apps,
        assigned_view_ids,
    })
}

#[tauri::command]
pub fn zone_mode_update(
    app: AppHandle,
    window: tauri::WebviewWindow,
    zones: Vec<ZoneRect>,
    state: tauri::State<'_, ZoneModeState>,
    hub: tauri::State<'_, HubControl>,
) -> Result<(), String> {
    let mut sessions = state.0.lock().map_err(|e| e.to_string())?;
    let label = window.label().to_string();
    if !sessions.contains_key(&label) {
        return Err("unknown Zone Mode overlay".to_string());
    }
    let previously_had_active = sessions
        .values()
        .flat_map(|session| session.zones.iter())
        .any(|zone| zone.trader_pool.as_deref() == Some("active"));
    if let Some(session) = sessions.get_mut(&label) {
        session.zones = zones;
    }
    let stole_active = take_exclusive_active(&mut sessions, &label);
    if stole_active && !previously_had_active {
        hub.set_monitor_active_pool(false);
        let _ = hub.unassign_monitor_trader_pool("zone-active-steal", "active");
    }
    let assigned_view_ids = assigned_zone_ids(&sessions, hub.monitor_active_pool());
    app.emit(
        "zone-mode-assignments-updated",
        serde_json::json!({ "assignedViewIds": assigned_view_ids }),
    )
    .map_err(|e| e.to_string())?;
    if hub.monitor_connected() {
        hub.configure_monitor_trader_pools("zone-pools-live", trader_pool_payload(&sessions))?;
    }
    Ok(())
}

#[tauri::command]
pub fn zone_mode_place(
    window: tauri::WebviewWindow,
    zone_id: String,
    view_id: String,
    state: tauri::State<'_, ZoneModeState>,
    hub: tauri::State<'_, HubControl>,
) -> Result<(), String> {
    if view_id == "sidebar" {
        return Err("Navigation cannot be zoned".to_string());
    }
    let sessions = state.0.lock().map_err(|e| e.to_string())?;
    let session = sessions
        .get(window.label())
        .ok_or_else(|| "unknown Zone Mode overlay".to_string())?;
    let zone = session
        .zones
        .iter()
        .find(|zone| zone.id == zone_id)
        .ok_or_else(|| "unknown zone".to_string())?;
    let bounds = serde_json::json!({
        "x": (session.origin_x + zone.x).round(),
        "y": (session.origin_y + zone.y).round(),
        "width": zone.width.round().max(1.0),
        "height": zone.height.round().max(1.0),
    });
    let request_id = format!(
        "zone-place-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default()
    );
    if let Ok(mut placements) = RECENT_PLACEMENTS.lock() {
        placements.retain(|_, placed_at| placed_at.elapsed() < Duration::from_secs(5));
        placements.insert(view_id.clone(), Instant::now());
    }
    let payload = serde_json::json!({ "viewId": view_id, "bounds": bounds });
    match zone.app_id.as_str() {
        "guilds" => hub.place_guilds_view(&request_id, payload),
        _ => hub.place_monitor_view(&request_id, payload),
    }
}

#[tauri::command]
pub fn zone_mode_close_view(
    view_id: String,
    app_id: Option<String>,
    hub: tauri::State<'_, HubControl>,
) -> Result<(), String> {
    let request_id = format!(
        "zone-close-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default()
    );
    match app_id.as_deref() {
        Some("guilds") => hub.close_guilds_view(&request_id, &view_id),
        _ => hub.close_monitor_view(&request_id, &view_id),
    }
}

#[tauri::command]
pub fn zone_mode_cancel(app: AppHandle, state: tauri::State<'_, ZoneModeState>) {
    if let Ok(mut sessions) = state.0.lock() {
        sessions.clear();
    }
    close_overlays_after_ipc(&app);
}

#[tauri::command]
pub fn zone_mode_finish(
    app: AppHandle,
    state: tauri::State<'_, ZoneModeState>,
) -> Result<Vec<PlacedZone>, String> {
    let sessions = state.0.lock().map_err(|e| e.to_string())?;
    save_layouts(&app, &sessions)?;
    let mut placed = Vec::new();
    for session in sessions.values() {
        for zone in &session.zones {
            placed.push(PlacedZone {
                display_id: session.display_id.clone(),
                x: session.origin_x + zone.x,
                y: session.origin_y + zone.y,
                width: zone.width,
                height: zone.height,
            });
        }
    }
    drop(sessions);

    eprintln!("[arcane-bridge] Zone Mode finished: {placed:?}");
    close_overlays_after_ipc(&app);
    Ok(placed)
}
