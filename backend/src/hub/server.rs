//! In-process TCP hub — one Arcane Bridge executable, no separate hub process.

use crate::bridge_admin::BridgeStatus;
use crate::hub::connections::{ConnectionRegistry, ConnId};
use crate::hub::io::write_bytes_to_stream;
use crate::hub::protocol::{handle_client_line, handle_monitor_publisher_line, ConnWriter, HubContext};
use crate::hub::state::HubState;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

struct HubRuntime {
    ctx: Arc<HubContext>,
    streams: Arc<Mutex<HashMap<ConnId, Arc<Mutex<TcpStream>>>>>,
}

impl HubRuntime {
    fn remove_conn(&self, conn: ConnId) {
        let was_monitor = {
            if let Ok(mut map) = self.streams.lock() {
                map.remove(&conn);
            }
            if let Ok(mut reg) = self.ctx.registry.lock() {
                reg.remove(conn)
            } else {
                false
            }
        };

        if let Ok(mut pending) = self.ctx.pending_by_request_id.lock() {
            pending.retain(|_, v| *v != conn);
        }

        if was_monitor {
            self.ctx.on_monitor_publisher_disconnect();
        } else {
            self.ctx.broadcast_admin_status();
        }
    }

    fn serve_connection(self: Arc<Self>, conn: ConnId, stream: TcpStream) {
        let _ = stream.set_read_timeout(None);
        let _ = stream.set_nodelay(true);

        let shared = Arc::new(Mutex::new(stream));
        if let Ok(mut map) = self.streams.lock() {
            map.insert(conn, Arc::clone(&shared));
        }

        let cloned = {
            let guard = match shared.lock() {
                Ok(g) => g,
                Err(_) => {
                    self.remove_conn(conn);
                    return;
                }
            };
            match guard.try_clone() {
                Ok(s) => s,
                Err(_) => {
                    drop(guard);
                    self.remove_conn(conn);
                    return;
                }
            }
        };

        let mut reader = BufReader::new(cloned);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let monitor_now = self
                        .ctx
                        .registry
                        .lock()
                        .ok()
                        .is_some_and(|r| r.monitor_publisher() == Some(conn));
                    if monitor_now {
                        handle_monitor_publisher_line(&self.ctx, conn, &line);
                    } else {
                        handle_client_line(&self.ctx, conn, &line);
                    }
                }
                Err(_) => break,
            }
        }

        self.remove_conn(conn);
    }
}

pub fn start(
    host: String,
    port: u16,
    version: String,
    status_tx: Sender<BridgeStatus>,
) -> Result<(), String> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    let listener = TcpListener::bind(addr).map_err(|e| {
        format!(
            "failed to bind bridge hub on {host}:{port} ({e}) — quit any old Arcane Bridge hub process and relaunch"
        )
    })?;

    let state = Arc::new(Mutex::new(HubState::default()));
    let registry = Arc::new(Mutex::new(ConnectionRegistry::new()));
    let pending = Arc::new(Mutex::new(HashMap::new()));
    let streams: Arc<Mutex<HashMap<ConnId, Arc<Mutex<TcpStream>>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let status_tx = Arc::new(status_tx);
    let host_for_ctx = host.clone();
    let notify_status = {
        let status_tx = Arc::clone(&status_tx);
        Arc::new(move |status: BridgeStatus| {
            let _ = status_tx.send(status);
        }) as Arc<dyn Fn(BridgeStatus) + Send + Sync>
    };

    let streams_for_write = Arc::clone(&streams);
    let write_bytes: ConnWriter = Arc::new(move |conn, bytes| {
        if let Ok(map) = streams_for_write.lock() {
            if let Some(stream) = map.get(&conn) {
                write_bytes_to_stream(stream, bytes);
            }
        }
    });

    let ctx = Arc::new(HubContext {
        host: host_for_ctx.clone(),
        port,
        version,
        state,
        registry,
        pending_by_request_id: pending,
        write_bytes,
        notify_status,
    });

    let runtime = Arc::new(HubRuntime {
        ctx: Arc::clone(&ctx),
        streams,
    });

    {
        let reg = ctx.registry.lock().map_err(|e| e.to_string())?;
        let initial = reg.connections_snapshot(&host_for_ctx, port);
        let _ = status_tx.send(initial);
    }

    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let conn = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);
            let rt = Arc::clone(&runtime);
            thread::spawn(move || rt.serve_connection(conn, stream));
        }
    });

    eprintln!("[arcane-bridge] hub listening on {host}:{port} (in-process)");
    Ok(())
}

pub fn probe_bridge_port(host: &str, port: u16) -> bool {
    let addr: SocketAddr = match format!("{host}:{port}").parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(800)).is_ok()
}
