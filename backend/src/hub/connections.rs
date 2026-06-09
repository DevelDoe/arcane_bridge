//! Track Arcane app roles on TCP connections for admin / tray UI.

use crate::bridge_admin::{BridgeAppsStatus, BridgeClient, BridgeStatus};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub type ConnId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientRole {
    Monitor,
    Caster,
    Guilds,
    BridgeApp,
    Unknown,
}

impl ClientRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Monitor => "monitor",
            Self::Caster => "caster",
            Self::Guilds => "guilds",
            Self::BridgeApp => "bridge_app",
            Self::Unknown => "unknown",
        }
    }
}

pub fn infer_role_from_client_id(id: Option<&str>) -> ClientRole {
    let s = id.unwrap_or("").trim().to_lowercase();
    if s.is_empty() {
        return ClientRole::Unknown;
    }
    if s.contains("monitor") {
        return ClientRole::Monitor;
    }
    if s.contains("caster") {
        return ClientRole::Caster;
    }
    if s.contains("guilds") {
        return ClientRole::Guilds;
    }
    if s.contains("bridge") || s.contains("admin") {
        return ClientRole::BridgeApp;
    }
    ClientRole::Unknown
}

#[derive(Clone)]
pub struct ClientMeta {
    pub role: ClientRole,
    pub client_id: String,
    pub connected_at: u64,
}

pub struct ConnectionRegistry {
    meta: HashMap<ConnId, ClientMeta>,
    watchlist_subscribers: HashSet<ConnId>,
    vault_subscribers: HashSet<ConnId>,
    caster_account_subscribers: HashSet<ConnId>,
    caster_journal_subscribers: HashSet<ConnId>,
    caster_ticket_subscribers: HashSet<ConnId>,
    admin_subscribers: HashSet<ConnId>,
    monitor_publisher: Option<ConnId>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self {
            meta: HashMap::new(),
            watchlist_subscribers: HashSet::new(),
            vault_subscribers: HashSet::new(),
            caster_account_subscribers: HashSet::new(),
            caster_journal_subscribers: HashSet::new(),
            caster_ticket_subscribers: HashSet::new(),
            admin_subscribers: HashSet::new(),
            monitor_publisher: None,
        }
    }

    pub fn tag(&mut self, id: ConnId, role: ClientRole, client_id: &str) {
        let entry = self.meta.entry(id).or_insert_with(|| ClientMeta {
            role,
            client_id: client_id.to_string(),
            connected_at: now_ms(),
        });
        entry.role = role;
        if !client_id.is_empty() {
            entry.client_id = client_id.to_string();
        }
    }

    pub fn remove(&mut self, id: ConnId) -> bool {
        self.watchlist_subscribers.remove(&id);
        self.vault_subscribers.remove(&id);
        self.caster_account_subscribers.remove(&id);
        self.caster_journal_subscribers.remove(&id);
        self.caster_ticket_subscribers.remove(&id);
        self.admin_subscribers.remove(&id);
        self.meta.remove(&id);
        if self.monitor_publisher == Some(id) {
            self.monitor_publisher = None;
            return true;
        }
        false
    }

    pub fn set_monitor_publisher(&mut self, id: ConnId) {
        self.monitor_publisher = Some(id);
    }

    pub fn monitor_publisher(&self) -> Option<ConnId> {
        self.monitor_publisher
    }

    pub fn clear_monitor_publisher(&mut self) {
        self.monitor_publisher = None;
    }

    pub fn watchlist_subscribers(&self) -> &HashSet<ConnId> {
        &self.watchlist_subscribers
    }

    pub fn vault_subscribers(&self) -> &HashSet<ConnId> {
        &self.vault_subscribers
    }

    pub fn caster_account_subscribers(&self) -> &HashSet<ConnId> {
        &self.caster_account_subscribers
    }

    pub fn caster_journal_subscribers(&self) -> &HashSet<ConnId> {
        &self.caster_journal_subscribers
    }

    pub fn caster_ticket_subscribers(&self) -> &HashSet<ConnId> {
        &self.caster_ticket_subscribers
    }

    pub fn admin_subscribers(&self) -> &HashSet<ConnId> {
        &self.admin_subscribers
    }

    pub fn watchlist_subscribers_mut(&mut self) -> &mut HashSet<ConnId> {
        &mut self.watchlist_subscribers
    }

    pub fn vault_subscribers_mut(&mut self) -> &mut HashSet<ConnId> {
        &mut self.vault_subscribers
    }

    pub fn caster_account_subscribers_mut(&mut self) -> &mut HashSet<ConnId> {
        &mut self.caster_account_subscribers
    }

    pub fn caster_journal_subscribers_mut(&mut self) -> &mut HashSet<ConnId> {
        &mut self.caster_journal_subscribers
    }

    pub fn caster_ticket_subscribers_mut(&mut self) -> &mut HashSet<ConnId> {
        &mut self.caster_ticket_subscribers
    }

    pub fn admin_subscribers_mut(&mut self) -> &mut HashSet<ConnId> {
        &mut self.admin_subscribers
    }

    pub fn connections_snapshot(&self, host: &str, port: u16) -> BridgeStatus {
        let mut seen: HashMap<ConnId, BridgeClient> = HashMap::new();

        if let Some(id) = self.monitor_publisher {
            if let Some(meta) = self.meta.get(&id) {
                seen.insert(
                    id,
                    BridgeClient {
                        role: ClientRole::Monitor.as_str().to_string(),
                        id: meta.client_id.clone(),
                        connected_at: Some(meta.connected_at),
                    },
                );
            }
        }

        for sub in &self.watchlist_subscribers {
            if seen.contains_key(sub) {
                continue;
            }
            if let Some(meta) = self.meta.get(sub) {
                seen.insert(
                    *sub,
                    BridgeClient {
                        role: meta.role.as_str().to_string(),
                        id: meta.client_id.clone(),
                        connected_at: Some(meta.connected_at),
                    },
                );
            }
        }

        let clients: Vec<BridgeClient> = seen.into_values().collect();
        let apps = BridgeAppsStatus {
            monitor: clients.iter().any(|c| c.role == "monitor"),
            caster: clients.iter().any(|c| c.role == "caster"),
            guilds: clients.iter().any(|c| c.role == "guilds"),
        };

        BridgeStatus {
            listening: true,
            host: host.to_string(),
            port,
            apps,
            clients,
        }
    }

    pub fn status_and_admin_payload(&self, host: &str, port: u16) -> (BridgeStatus, serde_json::Value) {
        let status = self.connections_snapshot(host, port);
        let payload = json!({
            "listening": status.listening,
            "host": status.host,
            "port": status.port,
            "apps": status.apps,
            "clients": status.clients,
        });
        (status, payload)
    }

    pub fn admin_payload(&self, host: &str, port: u16) -> serde_json::Value {
        self.status_and_admin_payload(host, port).1
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
