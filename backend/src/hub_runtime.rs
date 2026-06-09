//! Bridge runtime — env, singleton lock, in-process hub (no separate hub exe).

use std::net::SocketAddr;
use std::sync::mpsc::Receiver;

use crate::bridge_admin::BridgeStatus;
use crate::hub;

const DEFAULT_BRIDGE_PORT: u16 = 47991;
const DEFAULT_BRIDGE_HOST: &str = "127.0.0.1";
const SINGLETON_PORT: u16 = 47990;

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
    hub::probe_bridge_port(host, port)
}

/// Bind a local port so only one tray app runs at a time.
pub fn acquire_singleton_lock() -> Result<std::net::TcpListener, String> {
    let addr: SocketAddr = format!("{DEFAULT_BRIDGE_HOST}:{SINGLETON_PORT}")
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;
    std::net::TcpListener::bind(addr)
        .map_err(|e| format!("another Arcane Bridge tray is running ({e})"))
}

/// Start the TCP hub inside this process. Returns a channel of tray status updates.
pub fn start_in_process_hub() -> Result<Receiver<BridgeStatus>, String> {
    let host = bridge_host_from_env();
    let port = bridge_port_from_env();

    if probe_bridge_port(&host, port) {
        return Err(format!(
            "port {host}:{port} is already in use — quit any old Arcane Bridge or hub process and relaunch"
        ));
    }

    let (tx, rx) = std::sync::mpsc::channel();
    hub::start(host, port, tx)?;
    Ok(rx)
}
