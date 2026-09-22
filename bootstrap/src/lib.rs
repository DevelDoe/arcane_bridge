//! Launch Arcane Bridge if installed. Users install Bridge themselves (not bundled in apps).

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const BRIDGE_HOST: &str = "127.0.0.1";
const DEFAULT_BRIDGE_PORT: u16 = 47991;
const PRODUCT_NAME: &str = "Arcane Bridge";
#[cfg(target_os = "macos")]
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

/// Try once per process: if Bridge is installed, launch it and wait for the hub port.
/// Does not install or open bundled installers — users install Bridge like any other app.
pub fn ensure_bridge_running() -> Result<(), String> {
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

    if installed_binary().is_none() {
        eprintln!(
            "[bridge-bootstrap] {PRODUCT_NAME} not installed — install from GitHub Releases, then relaunch"
        );
        return Ok(());
    }

    eprintln!("[bridge-bootstrap] launching {PRODUCT_NAME}");
    launch_bridge()?;
    if wait_for_hub(&host, port, Duration::from_secs(45)) {
        return Ok(());
    }
    Err(format!("{PRODUCT_NAME} did not open hub on {host}:{port}"))
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

fn installed_binary() -> Option<PathBuf> {
    for candidate in installed_binary_candidates() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn installed_app_bundle_candidates() -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from("/Applications").join(APP_BUNDLE_NAME)];
    if let Ok(home) = std::env::var("HOME") {
        out.push(PathBuf::from(home).join("Applications").join(APP_BUNDLE_NAME));
    }
    out
}

#[cfg(target_os = "macos")]
fn installed_app_bundle() -> Option<PathBuf> {
    for candidate in installed_app_bundle_candidates() {
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn installed_binary_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();

    #[cfg(target_os = "macos")]
    {
        for app in installed_app_bundle_candidates() {
            out.push(app.join("Contents").join("MacOS").join(BIN_NAME));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let base = PathBuf::from(local);
            out.push(base.join(PRODUCT_NAME).join(format!("{BIN_NAME}.exe")));
            out.push(
                base.join("Programs")
                    .join(PRODUCT_NAME)
                    .join(format!("{BIN_NAME}.exe")),
            );
        }
    }

    #[cfg(target_os = "linux")]
    {
        out.push(PathBuf::from("/usr/bin").join(BIN_NAME));
        out.push(linux_user_install_root().join("usr/bin").join(BIN_NAME));
        if let Ok(home) = std::env::var("HOME") {
            out.push(PathBuf::from(home).join(".local/bin").join(BIN_NAME));
        }
    }

    out
}

#[cfg(target_os = "linux")]
fn linux_user_install_root() -> PathBuf {
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".local/share/arcane-bridge"))
        .unwrap_or_else(|_| PathBuf::from(".local/share/arcane-bridge"))
}

fn launch_bridge() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(app) = installed_app_bundle() {
            Command::new("open")
                .args(["-a"])
                .arg(&app)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| format!("launch {}: {e}", app.display()))?;
            return Ok(());
        }
        Command::new("open")
            .args(["-a", PRODUCT_NAME])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("launch {PRODUCT_NAME}: {e}"))?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let bin = installed_binary()
            .ok_or_else(|| format!("{PRODUCT_NAME} binary not found"))?;
        let mut cmd = Command::new(&bin);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.spawn()
            .map_err(|e| format!("launch {}: {e}", bin.display()))?;
        Ok(())
    }
}
