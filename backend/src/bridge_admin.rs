//! Subscribe to arcane_bridge `admin.*` messages for tray status.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::hub_process::{bridge_host_from_env, bridge_port_from_env};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BridgeAppsStatus {
    #[serde(default)]
    pub monitor: bool,
    #[serde(default)]
    pub caster: bool,
    #[serde(default)]
    pub guilds: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeClient {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub connected_at: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub listening: bool,
    pub host: String,
    pub port: u16,
    pub apps: BridgeAppsStatus,
    pub clients: Vec<BridgeClient>,
}

impl Default for BridgeStatus {
    fn default() -> Self {
        Self {
            listening: false,
            host: bridge_host_from_env(),
            port: bridge_port_from_env(),
            apps: BridgeAppsStatus::default(),
            clients: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AdminEnvelope {
    #[serde(rename = "type")]
    msg_type: String,
    payload: Option<serde_json::Value>,
}

fn parse_status(payload: &serde_json::Value) -> BridgeStatus {
    let host = payload
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let port = payload
        .get("port")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u16;
    let apps = payload
        .get("apps")
        .cloned()
        .and_then(|v| serde_json::from_value::<BridgeAppsStatus>(v).ok())
        .unwrap_or_default();
    let clients = payload
        .get("clients")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value::<BridgeClient>(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    BridgeStatus {
        listening: payload
            .get("listening")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        host: if host.is_empty() {
            bridge_host_from_env()
        } else {
            host
        },
        port: if port > 0 { port } else { bridge_port_from_env() },
        apps,
        clients,
    }
}

pub fn run_admin_subscribe_loop(tx: Sender<BridgeStatus>) {
    let host = bridge_host_from_env();
    let port = bridge_port_from_env();
    let mut backoff_ms = 500u64;

    loop {
        match admin_subscribe_session(&host, port, &tx) {
            Ok(()) => backoff_ms = 500,
            Err(e) => {
                let mut status = BridgeStatus::default();
                status.listening = crate::hub_process::probe_bridge_port(&host, port);
                let _ = tx.send(status);
                eprintln!("[arcane-bridge] admin subscribe: {e}");
            }
        }
        std::thread::sleep(Duration::from_millis(backoff_ms));
        backoff_ms = (backoff_ms.saturating_mul(2)).min(30_000);
    }
}

fn admin_subscribe_session(host: &str, port: u16, tx: &Sender<BridgeStatus>) -> Result<(), String> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(2_500))
        .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(None)
        .map_err(|e| e.to_string())?;

    let req = serde_json::json!({
        "schema": 1,
        "type": "admin.subscribe",
        "id": "arcane-bridge-tray",
        "payload": {}
    });
    stream
        .write_all(format!("{req}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let env: AdminEnvelope =
            serde_json::from_str(trimmed).map_err(|e| format!("parse admin line: {e}"))?;
        match env.msg_type.as_str() {
            "admin.snapshot" | "admin.update" | "admin.response" => {
                if let Some(payload) = env.payload {
                    let _ = tx.send(parse_status(&payload));
                }
            }
            "error" => {
                let msg = env
                    .payload
                    .as_ref()
                    .and_then(|p| p.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("bridge error");
                return Err(msg.to_string());
            }
            _ => {}
        }
    }

    Ok(())
}
