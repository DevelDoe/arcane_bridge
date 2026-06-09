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

enum InstallOutcome {
    #[cfg(not(target_os = "macos"))]
    Complete,
    NeedsUserInstall,
}

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

    if installed_binary().is_some() {
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

    eprintln!(
        "[bridge-bootstrap] installing {PRODUCT_NAME} from {}",
        installer.display()
    );
    match install_from_bundle(&installer)? {
        InstallOutcome::NeedsUserInstall => {
            eprintln!(
                "[bridge-bootstrap] opened Bridge installer — drag to Applications, then relaunch the app"
            );
            return Ok(());
        }
        #[cfg(not(target_os = "macos"))]
        InstallOutcome::Complete => {}
    }

    #[cfg(not(target_os = "macos"))]
    {
        if installed_binary().is_none() {
            return Err(format!(
                "install finished but {PRODUCT_NAME} binary was not found"
            ));
        }

        launch_bridge()?;
        if wait_for_hub(&host, port, Duration::from_secs(60)) {
            eprintln!("[bridge-bootstrap] {PRODUCT_NAME} hub is up on {host}:{port}");
            return Ok(());
        }

        return Err(format!(
            "{PRODUCT_NAME} installed but hub not listening on {host}:{port}"
        ));
    }
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
            out.push(
                base.join(PRODUCT_NAME)
                    .join(format!("{BIN_NAME}.exe")),
            );
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

    #[cfg(target_os = "macos")]
    let mut tarball: Option<PathBuf> = None;

    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let lower = name.to_lowercase();

        #[cfg(target_os = "windows")]
        if lower.ends_with(".exe") && lower.contains("bridge") {
            return Some(p);
        }

        #[cfg(target_os = "macos")]
        {
            if lower.ends_with(".dmg") && lower.contains("bridge") {
                return Some(p);
            }
            if lower.ends_with(".app.tar.gz") && lower.contains("bridge") {
                tarball = Some(p);
            }
        }

        #[cfg(target_os = "linux")]
        if lower.ends_with(".deb") && lower.contains("bridge") {
            return Some(p);
        }
    }

    #[cfg(target_os = "macos")]
    return tarball;

    #[cfg(not(target_os = "macos"))]
    None
}

fn install_from_bundle(installer: &Path) -> Result<InstallOutcome, String> {
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
        return Ok(InstallOutcome::Complete);
    }

    #[cfg(target_os = "macos")]
    {
        open_mac_installer(installer)?;
        return Ok(InstallOutcome::NeedsUserInstall);
    }

    #[cfg(target_os = "linux")]
    {
        let root = linux_user_install_root();
        if root.exists() {
            let _ = fs::remove_dir_all(&root);
        }
        fs::create_dir_all(&root).map_err(|e| format!("create install dir: {e}"))?;
        let status = Command::new("dpkg-deb")
            .args([
                "-x",
                installer.to_str().ok_or("installer path")?,
                root.to_str().ok_or("install root")?,
            ])
            .status()
            .map_err(|e| format!("dpkg-deb extract: {e}"))?;
        if !status.success() {
            return Err(format!("dpkg-deb exited with {status}"));
        }
        return Ok(InstallOutcome::Complete);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = installer;
        Err("bridge piggyback install is not supported on this platform".into())
    }
}

#[cfg(target_os = "macos")]
fn open_mac_installer(installer: &Path) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let downloads = PathBuf::from(home).join("Downloads");
    fs::create_dir_all(&downloads).map_err(|e| format!("create downloads dir: {e}"))?;

    let file_name = installer
        .file_name()
        .ok_or("installer path has no file name")?;
    let dest = downloads.join(file_name);
    fs::copy(installer, &dest).map_err(|e| format!("copy installer to downloads: {e}"))?;

    let _ = Command::new("xattr")
        .args(["-d", "com.apple.quarantine"])
        .arg(&dest)
        .status();

    let status = Command::new("open")
        .arg(&dest)
        .status()
        .map_err(|e| format!("open installer: {e}"))?;
    if !status.success() {
        return Err(format!("open installer exited with {status}"));
    }

    eprintln!(
        "[bridge-bootstrap] copied Bridge installer to {} — drag to Applications, then relaunch",
        dest.display()
    );
    Ok(())
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
        let bin = installed_binary().ok_or_else(|| {
            format!("{PRODUCT_NAME} binary not found after install")
        })?;
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
