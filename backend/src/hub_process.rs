//! Spawn bundled arcane_bridge hub when nothing is listening on the bridge port.

use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use tauri::Manager;

const DEFAULT_BRIDGE_PORT: u16 = 47991;
const DEFAULT_BRIDGE_HOST: &str = "127.0.0.1";
const SINGLETON_PORT: u16 = 47990;
const CONNECT_TIMEOUT_MS: u64 = 800;
const BUNDLED_HUB_DIR: &str = "hub";
const HUB_EXE_NAMES: &[&str] = &["arcane-bridge-hub", "arcane-bridge-hub.exe"];

static HUB_CHILD: Mutex<Option<Child>> = Mutex::new(None);
static BUNDLED_HUB_ENTRY: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn bridge_host_from_env() -> String {
    std::env::var("ARCANE_BRIDGE_HOST")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BRIDGE_HOST.to_string())
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
    TcpStream::connect_timeout(&addr, Duration::from_millis(CONNECT_TIMEOUT_MS)).is_ok()
}

/// Bind a local port so only one tray app runs at a time.
pub fn acquire_singleton_lock() -> Result<std::net::TcpListener, String> {
    let addr: SocketAddr = format!("{DEFAULT_BRIDGE_HOST}:{SINGLETON_PORT}")
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;
    std::net::TcpListener::bind(addr)
        .map_err(|e| format!("another Arcane Bridge tray is running ({e})"))
}

fn find_hub_exe_in(dir: &Path) -> Option<PathBuf> {
    for name in HUB_EXE_NAMES {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Resolve bundled hub path from Tauri resource dir (production builds).
pub fn init_bundled_hub_paths(app: &tauri::AppHandle) {
    let mut found = None;

    if let Ok(dir) = app.path().resource_dir() {
        found = find_hub_exe_in(&dir.join(BUNDLED_HUB_DIR));
    }

    if found.is_none() {
        found = resource_hub_exe_near_exe();
    }

    if let Some(path) = found {
        if let Ok(mut guard) = BUNDLED_HUB_ENTRY.lock() {
            *guard = Some(path);
        }
    }
}

fn resource_hub_exe_near_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;

    #[cfg(target_os = "macos")]
    {
        let candidate = exe
            .parent()?
            .parent()?
            .join("Resources")
            .join(BUNDLED_HUB_DIR);
        if let Some(found) = find_hub_exe_in(&candidate) {
            return Some(found);
        }
    }

    let parent = exe.parent()?;
    for hub_dir in [
        parent.join(BUNDLED_HUB_DIR),
        parent.join("resources").join(BUNDLED_HUB_DIR),
    ] {
        if let Some(found) = find_hub_exe_in(&hub_dir) {
            return Some(found);
        }
    }

    None
}

fn entry_is_script(entry: &Path) -> bool {
    entry
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "js" | "cjs" | "mjs"))
}

fn resolve_bridge_entry() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("ARCANE_BRIDGE_ENTRY") {
        let p = PathBuf::from(raw.trim());
        if p.is_file() {
            return Some(p);
        }
    }

    if let Ok(guard) = BUNDLED_HUB_ENTRY.lock() {
        if let Some(path) = guard.as_ref() {
            if path.is_file() {
                return Some(path.clone());
            }
        }
    }

    if let Some(path) = resource_hub_exe_near_exe() {
        return Some(path);
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_candidates = [
        manifest.join("../hub/dist/hub-bundle/arcane-bridge-hub"),
        manifest.join("../hub/dist/hub-bundle/arcane-bridge-hub.exe"),
        manifest.join("../hub/dist/arcane-bridge.cjs"),
        manifest.join("../hub/src/index.js"),
    ];
    for path in dev_candidates {
        if path.is_file() {
            return Some(path);
        }
    }

    None
}

fn hub_working_dir(entry: &Path) -> PathBuf {
    entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn build_hub_command(entry: &Path, host: &str, port: u16) -> Command {
    let cwd = hub_working_dir(entry);
    let mut cmd = if entry_is_script(entry) {
        let node = std::env::var("ARCANE_BRIDGE_NODE").unwrap_or_else(|_| "node".to_string());
        let mut script_cmd = Command::new(node);
        script_cmd.arg(entry);
        script_cmd
    } else {
        Command::new(entry)
    };

    cmd.current_dir(cwd)
        .env("ARCANE_BRIDGE_HOST", host)
        .env("ARCANE_BRIDGE_PORT", port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd
}

/// Start hub if port is down. Returns true when this process spawned the child.
pub fn ensure_hub_running() -> Result<bool, String> {
    let host = bridge_host_from_env();
    let port = bridge_port_from_env();

    if probe_bridge_port(&host, port) {
        eprintln!("[arcane-bridge] hub already listening on {host}:{port}");
        return Ok(false);
    }

    let entry = resolve_bridge_entry().ok_or_else(|| {
        "hub executable not found — reinstall Arcane Bridge or set ARCANE_BRIDGE_ENTRY".to_string()
    })?;

    if entry_is_script(&entry) {
        eprintln!("[arcane-bridge] dev hub script {:?} (requires Node on PATH)", entry);
    } else {
        eprintln!("[arcane-bridge] launching bundled hub {:?}", entry);
    }

    let mut child = build_hub_command(&entry, &host, port)
        .spawn()
        .map_err(|e| format!("failed to spawn hub ({:?}): {e}", entry))?;

    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let t = line.trim();
                if !t.is_empty() {
                    eprintln!("[arcane-bridge] {t}");
                }
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let t = line.trim();
                if !t.is_empty() {
                    eprintln!("[arcane-bridge] {t}");
                }
            }
        });
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    while std::time::Instant::now() < deadline {
        if probe_bridge_port(&host, port) {
            let mut guard = HUB_CHILD.lock().map_err(|e| e.to_string())?;
            *guard = Some(child);
            eprintln!("[arcane-bridge] spawned hub on {host}:{port}");
            return Ok(true);
        }
        if child.try_wait().ok().flatten().is_some() {
            return Err("hub process exited before port was ready".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    let _ = child.kill();
    Err(format!("hub did not listen on {host}:{port} within 12s"))
}

pub fn stop_spawned_hub() {
    let mut guard = match HUB_CHILD.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
        eprintln!("[arcane-bridge] stopped spawned hub");
    }
}
