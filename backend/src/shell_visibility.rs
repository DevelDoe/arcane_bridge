//! Keep Arcane Bridge tray-only — no Dock icon, no taskbar entry, no Windows console.

use tauri::AppHandle;

/// Run before Tauri starts on Windows so dev/release builds never attach a console.
pub fn prepare_windows_tray_process() {
    #[cfg(windows)]
    detach_from_console();
}

pub fn apply_tray_only_shell(_app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = _app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        let _ = _app.set_dock_visibility(false);
    }
}

#[cfg(windows)]
fn detach_from_console() {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn ShowWindow(hwnd: *mut c_void, n_cmd_show: i32) -> i32;
        fn FreeConsole() -> i32;
    }

    const SW_HIDE: i32 = 0;

    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
        let _ = FreeConsole();
    }
}
