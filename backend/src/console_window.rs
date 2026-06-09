//! Bridge Console — live hub status window.

use serde::Serialize;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::activity_log::ActivityLog;
use crate::shell_visibility::apply_tray_only_shell;
use crate::bridge_admin::{BridgeAppsStatus, BridgeClient, BridgeStatus};

pub const CONSOLE_LABEL: &str = "bridge-console";
const CONSOLE_EVENT: &str = "bridge-console-update";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleSnapshot {
    pub listening: bool,
    pub host: String,
    pub port: u16,
    pub apps: BridgeAppsStatus,
    pub clients: Vec<BridgeClient>,
    pub activity: Vec<String>,
    pub version: String,
}

pub struct ConsoleState {
    pub status: BridgeStatus,
    pub activity: ActivityLog,
    pub version: String,
}

impl ConsoleState {
    pub fn new(version: String, initial: BridgeStatus) -> Self {
        let mut activity = ActivityLog::default();
        activity.record_status(None, &initial);
        Self {
            status: initial,
            activity,
            version,
        }
    }

    pub fn apply_status(&mut self, status: BridgeStatus) {
        self.activity.record_status(Some(&self.status), &status);
        self.status = status;
    }

    pub fn snapshot(&self) -> ConsoleSnapshot {
        ConsoleSnapshot {
            listening: self.status.listening,
            host: self.status.host.clone(),
            port: self.status.port,
            apps: self.status.apps.clone(),
            clients: self.status.clients.clone(),
            activity: self.activity.lines().to_vec(),
            version: self.version.clone(),
        }
    }
}

pub type SharedConsoleState = Arc<Mutex<ConsoleState>>;

pub fn open_console(app: &AppHandle, state: &SharedConsoleState) -> Result<(), String> {
    let snapshot = state
        .lock()
        .map_err(|e| format!("console state lock: {e}"))?
        .snapshot();

    if let Some(win) = app.get_webview_window(CONSOLE_LABEL) {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        win.emit(CONSOLE_EVENT, &snapshot)
            .map_err(|e| e.to_string())?;
        apply_tray_only_shell(app);
        return Ok(());
    }

    let app_for_events = app.clone();
    let win = WebviewWindowBuilder::new(app, CONSOLE_LABEL, WebviewUrl::App("console.html".into()))
        .title("Bridge Console")
        .inner_size(520.0, 440.0)
        .min_inner_size(400.0, 320.0)
        .resizable(true)
        .skip_taskbar(true)
        .build()
        .map_err(|e| format!("create bridge console: {e}"))?;

    win.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
        ) {
            apply_tray_only_shell(&app_for_events);
        }
    });

    let _ = win.emit(CONSOLE_EVENT, &snapshot);
    apply_tray_only_shell(app);

    // Webview may attach its listener slightly after first paint.
    let app_retry = app.clone();
    let snapshot_retry = snapshot;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let _ = app_retry.emit_to(CONSOLE_LABEL, CONSOLE_EVENT, &snapshot_retry);
    });

    Ok(())
}

pub fn emit_console_update(app: &AppHandle, state: &SharedConsoleState) {
    if app.get_webview_window(CONSOLE_LABEL).is_none() {
        return;
    }
    let Ok(guard) = state.lock() else {
        return;
    };
    let snapshot = guard.snapshot();
    drop(guard);
    let _ = app.emit_to(CONSOLE_LABEL, CONSOLE_EVENT, &snapshot);
}
