//! Piggyback install + launch Arcane Bridge before apps connect to the hub.

use std::fs;
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

    if let Some(bin) = installed_binary() {
        eprintln!("[bridge-bootstrap] launching {PRODUCT_NAME}");
        launch_binary(&bin)?;
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

    eprintln!(
        "[bridge-bootstrap] installing {PRODUCT_NAME} from {}",
        installer.display()
    );
    install_from_bundle(&installer)?;

    let Some(bin) = installed_binary() else {
        return Err(format!(
            "install finished but {PRODUCT_NAME} binary was not found"
        ));
    };

    launch_binary(&bin)?;
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

fn installed_binary() -> Option<PathBuf> {
    for candidate in installed_binary_candidates() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn installed_binary_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();

    #[cfg(target_os = "macos")]
    {
        out.push(
            PathBuf::from("/Applications")
                .join(APP_BUNDLE_NAME)
                .join("Contents/MacOS")
                .join(BIN_NAME),
        );
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(exe) = windows_install_exe() {
            out.push(exe);
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

#[cfg(target_os = "windows")]
fn windows_install_exe() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("Programs")
            .join(PRODUCT_NAME)
            .join(format!("{BIN_NAME}.exe")),
    )
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

    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };

    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let lower = name.to_lowercase();

        #[cfg(target_os = "windows")]
        if lower.ends_with(".exe") && lower.contains("bridge") {
            return Some(p);
        }

        #[cfg(target_os = "macos")]
        if lower.ends_with(".app.tar.gz") && lower.contains("bridge") {
            return Some(p);
        }

        #[cfg(target_os = "linux")]
        if lower.ends_with(".deb") && lower.contains("bridge") {
            return Some(p);
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

    #[cfg(target_os = "linux")]
    {
        let root = linux_user_install_root();
        if root.exists() {
            let _ = fs::remove_dir_all(&root);
        }
        fs::create_dir_all(&root).map_err(|e| format!("create install dir: {e}"))?;
        let status = Command::new("dpkg-deb")
            .args(["-x", installer.to_str().ok_or("installer path")?, root.to_str().ok_or("install root")?])
            .status()
            .map_err(|e| format!("dpkg-deb extract: {e}"))?;
        if !status.success() {
            return Err(format!("dpkg-deb exited with {status}"));
        }
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = installer;
        Err("bridge piggyback install is not supported on this platform".into())
    }
}

fn launch_binary(bin: &Path) -> Result<(), String> {
    Command::new(bin)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("launch {}: {e}", bin.display()))?;
    Ok(())
}
