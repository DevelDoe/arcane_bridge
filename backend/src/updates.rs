//! In-app updates via tauri-plugin-updater (GitHub Releases JSON).

use tauri::AppHandle;

#[cfg(desktop)]
use tauri_plugin_updater::UpdaterExt;

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

#[cfg(not(desktop))]
pub async fn check_and_install(_app: &AppHandle) -> Result<(), String> {
    Err("Updates are not available on this platform.".into())
}
