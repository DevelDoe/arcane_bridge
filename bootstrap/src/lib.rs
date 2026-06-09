//! Piggyback install + launch Arcane Bridge before apps connect to the hub.

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const BRIDGE_HOST: &str = "127.0.0.1";
const DEFAULT_BRIDGE_PORT: u16 = 47991;
const PRODUCT_NAME: &str = "Arcane Bridge";
const APP_BUNDLE_NAME: &str = "Arcane Bridge.app";
const BIN_NAME: &str = "arcane-bridge";

static BOOTSTRAP_ATTEMPTED: AtomicBool = AtomicBool::new(false);

pub fn bridge_port_from_env() -> u16 {
    std::env::var("ARCANE_BRIDGE_PORT")
        .or_else(|_| std::env::var("ARCANE_GUILD_BRIDGE_PORT"))
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|&p| p > 0)
        .unwrap_or(DEFAULT_BRIDGE_PORT)
}

pub fn probe_bridge_port(host: &str, port: u16) -> bool {
    let addr: SocketAddr = match format!("{host}:{port}").parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(800)).is_ok()
}

/// Try once per process: install bundled Bridge if needed, launch tray, wait for hub port.
pub fn ensure_bridge_running(resource_dir: Option<&Path>) -> Result<(), String> {
    if BOOTSTRAP_ATTEMPTED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let host = std::env::var("ARCANE_BRIDGE_HOST")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| BRIDGE_HOST.to_string());
    let port = bridge_port_from_env();

    if probe_bridge_port(&host, port) {
        eprintln!("[bridge-bootstrap] hub already on {host}:{port}");
        return Ok(());
    }

    if bridge_is_installed() {
        eprintln!("[bridge-bootstrap] launching {PRODUCT_NAME}");
        launch_bridge()?;
        if wait_for_hub(&host, port, Duration::from_secs(45)) {
            return Ok(());
        }
        return Err(format!("{PRODUCT_NAME} did not open hub on {host}:{port}"));
    }

    let Some(installer) = find_bundled_installer(resource_dir) else {
        eprintln!(
            "[bridge-bootstrap] {PRODUCT_NAME} not installed and no bundled installer — start Bridge manually"
        );
        return Ok(());
    };

    eprintln!("[bridge-bootstrap] installing {PRODUCT_NAME} from {}", installer.display());
    install_from_bundle(&installer)?;

    if !bridge_is_installed() {
        return Err(format!(
            "install finished but {PRODUCT_NAME} was not found at expected path"
        ));
    }

    launch_bridge()?;
    if wait_for_hub(&host, port, Duration::from_secs(60)) {
        eprintln!("[bridge-bootstrap] {PRODUCT_NAME} hub is up on {host}:{port}");
        return Ok(());
    }

    Err(format!(
        "{PRODUCT_NAME} installed but hub not listening on {host}:{port}"
    ))
}

fn wait_for_hub(host: &str, port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if probe_bridge_port(host, port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn bridge_is_installed() -> bool {
    installed_app_path().is_some()
}

fn installed_app_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let app = PathBuf::from("/Applications").join(APP_BUNDLE_NAME);
        if app.join("Contents/MacOS").join(BIN_NAME).is_file() {
            return Some(app);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(exe) = windows_install_exe() {
            if exe.is_file() {
                return Some(exe);
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = ();
    }

    None
}

#[cfg(target_os = "windows")]
fn windows_install_exe() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let exe = PathBuf::from(local)
        .join("Programs")
        .join(PRODUCT_NAME)
        .join(format!("{BIN_NAME}.exe"));
    Some(exe)
}

fn find_bundled_installer(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = resource_dir {
        dirs.push(dir.join("bridge"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("resources").join("bridge"));
            #[cfg(target_os = "macos")]
            {
                if let Some(contents) = parent.parent() {
                    dirs.push(contents.join("Resources").join("bridge"));
                }
            }
        }
    }

    for dir in dirs {
        if let Some(found) = pick_installer_in_dir(&dir) {
            return Some(found);
        }
    }
    None
}

fn pick_installer_in_dir(dir: &Path) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        for name in ["arcane-bridge-setup.exe", "Arcane Bridge-setup.exe"] {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("exe")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.to_lowercase().contains("bridge"))
                {
                    return Some(p);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for name in ["Arcane-Bridge.app.tar.gz", "arcane-bridge.app.tar.gz"] {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    None
}

fn install_from_bundle(installer: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new(installer)
            .arg("/S")
            .status()
            .map_err(|e| format!("run installer: {e}"))?;
        if !status.success() {
            return Err(format!("installer exited with {status}"));
        }
        std::thread::sleep(Duration::from_secs(3));
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let dest = PathBuf::from("/Applications");
        let status = Command::new("tar")
            .args(["xzf", installer.to_str().ok_or("installer path")?, "-C"])
            .arg(&dest)
            .status()
            .map_err(|e| format!("tar extract: {e}"))?;
        if !status.success() {
            return Err(format!("tar exited with {status}"));
        }
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = installer;
        Err("bridge piggyback install is only supported on macOS and Windows".into())
    }
}

fn launch_bridge() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(app) = installed_app_path() {
            Command::new("open")
                .arg("-a")
                .arg(&app)
                .spawn()
                .map_err(|e| format!("open bridge app: {e}"))?;
            return Ok(());
        }
        Command::new("open")
            .arg("-a")
            .arg(PRODUCT_NAME)
            .spawn()
            .map_err(|e| format!("open bridge: {e}"))?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let exe = windows_install_exe().ok_or("windows bridge path unknown")?;
        Command::new(&exe)
            .spawn()
            .map_err(|e| format!("launch bridge: {e}"))?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("launch bridge unsupported on this platform".into())
    }
}
