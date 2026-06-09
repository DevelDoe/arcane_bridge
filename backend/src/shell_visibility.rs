//! Keep Arcane Bridge tray-only — no Dock icon, no taskbar entry.

use tauri::AppHandle;

pub fn apply_tray_only_shell(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        let _ = app.set_dock_visibility(false);
    }
}
