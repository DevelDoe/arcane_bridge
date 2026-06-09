// Tray app must never be a console process on Windows — a visible console ties
// the hub lifecycle to that window (closing it kills the hub).
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    app_lib::prepare_windows_tray_process();
    app_lib::run();
}
