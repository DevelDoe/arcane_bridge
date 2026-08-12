//! In-app updates via tauri-plugin-updater (GitHub Releases JSON).
//! Auto-check on boot and every hour; tray menu can still trigger a manual check.

use std::time::Duration;

use tauri::AppHandle;

#[cfg(desktop)]
use tauri_plugin_updater::UpdaterExt;

const AUTO_UPDATE_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[cfg(desktop)]
pub async fn check_and_install(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        eprintln!("[arcane-bridge] update: already on latest");
        return Ok(());
    };

    eprintln!(
        "[arcane-bridge] update: downloading v{}",
        update.version
    );

    update
        .download_and_install(
            |chunk_len, content_len| {
                if let Some(total) = content_len {
                    eprintln!("[arcane-bridge] update: {chunk_len}/{total} bytes");
                }
            },
            || {
                eprintln!("[arcane-bridge] update: install complete, restarting…");
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Boot check, then poll once per hour. Silent when already up to date.
#[cfg(desktop)]
pub fn spawn_auto_updater(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(e) = check_and_install(&app).await {
                eprintln!("[arcane-bridge] auto-update: {e}");
            }
            tokio::time::sleep(AUTO_UPDATE_INTERVAL).await;
        }
    });
}

#[cfg(not(desktop))]
pub async fn check_and_install(_app: &AppHandle) -> Result<(), String> {
    Err("Updates are not available on this platform.".into())
}

#[cfg(not(desktop))]
pub fn spawn_auto_updater(_app: AppHandle) {}
