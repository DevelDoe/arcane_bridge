//! Bridge hub status types for tray UI and admin snapshots.

use serde::{Deserialize, Serialize};

use crate::hub_runtime::{bridge_host_from_env, bridge_port_from_env};

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
