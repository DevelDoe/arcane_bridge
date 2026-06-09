//! Rolling activity lines for Bridge Console.

use std::collections::HashSet;

use crate::bridge_admin::{BridgeClient, BridgeStatus};

const MAX_LINES: usize = 80;

#[derive(Debug, Clone, Default)]
pub struct ActivityLog {
    lines: Vec<String>,
}

impl ActivityLog {
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn push(&mut self, line: impl Into<String>) {
        let stamp = utc_hms();
        self.lines.push(format!("[{stamp}] {}", line.into()));
        if self.lines.len() > MAX_LINES {
            let drop = self.lines.len() - MAX_LINES;
            self.lines.drain(0..drop);
        }
    }

    pub fn record_status(&mut self, prev: Option<&BridgeStatus>, next: &BridgeStatus) {
        if let Some(prev) = prev {
            if prev.listening != next.listening {
                if next.listening {
                    self.push(format!("Hub online at {}:{}", next.host, next.port));
                } else {
                    self.push("Hub offline — reconnecting…");
                }
            }

            let prev_keys: HashSet<String> = prev.clients.iter().map(client_key).collect();
            let next_keys: HashSet<String> = next.clients.iter().map(client_key).collect();

            for client in &next.clients {
                if !prev_keys.contains(&client_key(client)) {
                    self.push(format!(
                        "{} connected ({})",
                        role_label(&client.role),
                        client.id
                    ));
                }
            }
            for client in &prev.clients {
                if !next_keys.contains(&client_key(client)) {
                    self.push(format!(
                        "{} disconnected ({})",
                        role_label(&client.role),
                        client.id
                    ));
                }
            }
        } else if next.listening {
            self.push(format!("Hub online at {}:{}", next.host, next.port));
        } else {
            self.push("Waiting for hub…");
        }
    }
}

fn client_key(client: &BridgeClient) -> String {
    format!("{}:{}", client.role, client.id)
}

fn role_label(role: &str) -> &'static str {
    match role {
        "monitor" => "Monitor",
        "caster" => "Caster",
        "guilds" => "Guilds",
        "bridge_app" => "Bridge",
        _ => "Client",
    }
}

fn utc_hms() -> String {
    let Ok(duration) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return String::from("--:--:--");
    };
    let total = duration.as_secs();
    let h = (total / 3600) % 24;
    let m = (total / 60) % 60;
    let s = total % 60;
    format!("{h:02}:{m:02}:{s:02}")
}
