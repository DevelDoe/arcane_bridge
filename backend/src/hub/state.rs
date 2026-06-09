//! In-memory hub state — Monitor publishes; Caster publishes feed focus.

use serde_json::{json, Value};

#[derive(Default)]
struct WatchlistState {
    symbols: Vec<String>,
    manual_focus_symbol: Option<String>,
    feed_focus_symbol: Option<String>,
    active_stocks: Vec<Value>,
    active_ticker: Option<String>,
    active_hero_mode_enabled: bool,
    publisher_user_id: Option<String>,
}

#[derive(Default)]
struct VaultState {
    symbols: Vec<String>,
    publisher_user_id: Option<String>,
}

pub struct HubState {
    watchlist: WatchlistState,
    vault: VaultState,
    caster_account: Option<Value>,
    caster_journal: Option<Value>,
    guilds_feed: Option<Value>,
    monitor_token: Option<String>,
}

impl Default for HubState {
    fn default() -> Self {
        Self {
            watchlist: WatchlistState {
                active_hero_mode_enabled: true,
                ..Default::default()
            },
            vault: VaultState::default(),
            caster_account: None,
            caster_journal: None,
            guilds_feed: None,
            monitor_token: None,
        }
    }
}

fn normalize_symbol(raw: Option<&Value>) -> Option<String> {
    let t = raw
        .map(|v| match v {
            Value::String(s) => s.trim().to_uppercase(),
            _ => v.to_string().trim().to_uppercase(),
        })
        .unwrap_or_default();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn normalize_symbols(arr: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(items)) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in items {
        if let Some(s) = normalize_symbol(Some(raw)) {
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
    }
    out
}

impl HubState {
    pub fn watchlist_payload(&self) -> Value {
        let w = &self.watchlist;
        let mut payload = json!({
            "symbols": w.symbols,
            "manualFocusSymbol": w.manual_focus_symbol,
            "activeStocks": w.active_stocks,
            "activeTicker": w.active_ticker,
            "activeHeroModeEnabled": w.active_hero_mode_enabled,
        });
        if let Some(sym) = &w.feed_focus_symbol {
            payload["feedFocusSymbol"] = json!(sym);
        }
        if let Some(uid) = &w.publisher_user_id {
            payload["publisherUserId"] = json!(uid);
        }
        payload
    }

    pub fn vault_payload(&self) -> Value {
        let v = &self.vault;
        let mut payload = json!({ "symbols": v.symbols });
        if let Some(uid) = &v.publisher_user_id {
            payload["publisherUserId"] = json!(uid);
        }
        payload
    }

    pub fn caster_account_payload(&self) -> Option<Value> {
        self.caster_account.clone()
    }

    pub fn merge_caster_account_publish(&mut self, payload: &Value) {
        if payload.is_object() {
            self.caster_account = Some(payload.clone());
        }
    }

    pub fn caster_journal_payload(&self) -> Option<Value> {
        self.caster_journal.clone()
    }

    pub fn merge_caster_journal_publish(&mut self, payload: &Value) {
        if payload.is_object() {
            self.caster_journal = Some(payload.clone());
        }
    }

    pub fn guilds_feed_payload(&self) -> Option<Value> {
        self.guilds_feed.clone()
    }

    pub fn merge_guilds_feed_publish(&mut self, payload: &Value) {
        if payload.is_object() {
            self.guilds_feed = Some(payload.clone());
        }
    }

    pub fn merge_watchlist_publish(&mut self, payload: &Value) {
        let Some(obj) = payload.as_object() else {
            return;
        };
        let w = &mut self.watchlist;
        if obj.contains_key("symbols") {
            w.symbols = normalize_symbols(obj.get("symbols"));
        }
        if obj.contains_key("manualFocusSymbol") {
            w.manual_focus_symbol = normalize_symbol(obj.get("manualFocusSymbol"));
        }
        if obj.contains_key("feedFocusSymbol") {
            w.feed_focus_symbol = normalize_symbol(obj.get("feedFocusSymbol"));
        }
        if let Some(Value::Array(stocks)) = obj.get("activeStocks") {
            w.active_stocks = stocks.clone();
        }
        if obj.contains_key("activeTicker") {
            w.active_ticker = normalize_symbol(obj.get("activeTicker"));
        }
        if obj.contains_key("activeHeroModeEnabled") {
            w.active_hero_mode_enabled = obj
                .get("activeHeroModeEnabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }
        if let Some(uid) = obj.get("publisherUserId").and_then(|v| v.as_str()) {
            let trimmed = uid.trim();
            if !trimmed.is_empty() {
                w.publisher_user_id = Some(trimmed.to_string());
            }
        }
    }

    pub fn merge_vault_publish(&mut self, payload: &Value) {
        let Some(obj) = payload.as_object() else {
            return;
        };
        if obj.contains_key("symbols") {
            self.vault.symbols = normalize_symbols(obj.get("symbols"));
        }
        if let Some(uid) = obj.get("publisherUserId").and_then(|v| v.as_str()) {
            let trimmed = uid.trim();
            if !trimmed.is_empty() {
                self.vault.publisher_user_id = Some(trimmed.to_string());
            }
        }
    }

    pub fn set_feed_focus_symbol(&mut self, sym: Option<String>) {
        self.watchlist.feed_focus_symbol = sym;
    }

    pub fn set_monitor_token(&mut self, token: Option<String>) {
        self.monitor_token = token.filter(|t| !t.trim().is_empty());
    }

    pub fn monitor_token(&self) -> Option<&str> {
        self.monitor_token.as_deref()
    }

    pub fn clear_monitor_publisher_state(&mut self) {
        self.watchlist = WatchlistState {
            active_hero_mode_enabled: true,
            ..Default::default()
        };
        self.vault = VaultState::default();
        self.caster_account = None;
        self.caster_journal = None;
        self.guilds_feed = None;
        self.monitor_token = None;
    }
}
