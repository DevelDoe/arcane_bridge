//! JSONL bridge protocol for Monitor, Guilds, and Caster clients.

use crate::bridge_admin::BridgeStatus;
use crate::hub::connections::{infer_role_from_client_id, ClientRole, ConnId, ConnectionRegistry};
use crate::hub::io::{encode_json_line, encode_line_with_payload};
use crate::hub::state::HubState;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(crate) type ConnWriter = Arc<dyn Fn(ConnId, &[u8]) + Send + Sync>;

pub struct HubContext {
    pub host: String,
    pub port: u16,
    pub version: String,
    pub state: Arc<Mutex<HubState>>,
    pub registry: Arc<Mutex<ConnectionRegistry>>,
    pub pending_by_request_id: Arc<Mutex<HashMap<String, ConnId>>>,
    pub(crate) write_bytes: ConnWriter,
    pub(crate) notify_status: Arc<dyn Fn(BridgeStatus) + Send + Sync>,
}

impl HubContext {
    fn enrich_admin_payload(&self, mut payload: Value) -> Value {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("version".to_string(), json!(self.version));
        }
        payload
    }

    fn write(&self, conn: ConnId, obj: &Value) {
        if let Some(bytes) = encode_json_line(obj) {
            (self.write_bytes)(conn, &bytes);
        }
    }

    fn write_raw_line(&self, conn: ConnId, line: &str) {
        let mut bytes = line.as_bytes().to_vec();
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        (self.write_bytes)(conn, &bytes);
    }

    fn error_line(id: Option<&str>, code: &str, message: &str) -> Value {
        json!({
            "schema": 1,
            "type": "error",
            "id": id,
            "payload": { "code": code, "message": message }
        })
    }

    fn fanout(&self, msg_type: &str, payload: Value, subscribers: &[ConnId]) {
        let Some(bytes) = encode_line_with_payload(msg_type, payload) else {
            return;
        };
        for id in subscribers {
            (self.write_bytes)(*id, &bytes);
        }
    }

    fn subscriber_ids<F>(&self, pick: F) -> Vec<ConnId>
    where
        F: FnOnce(&ConnectionRegistry) -> &std::collections::HashSet<ConnId>,
    {
        self.registry
            .lock()
            .map(|r| pick(&r).iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn broadcast_watchlist(&self) {
        let subs = self.subscriber_ids(|r| r.watchlist_subscribers());
        if subs.is_empty() {
            return;
        }
        let payload = self
            .state
            .lock()
            .map(|s| s.watchlist_payload())
            .unwrap_or(json!({}));
        self.fanout("watchlist.update", payload, &subs);
    }

    pub fn broadcast_vault(&self) {
        let subs = self.subscriber_ids(|r| r.vault_subscribers());
        if subs.is_empty() {
            return;
        }
        let payload = self
            .state
            .lock()
            .map(|s| s.vault_payload())
            .unwrap_or(json!({}));
        self.fanout("vault.update", payload, &subs);
    }

    pub fn broadcast_caster_account(&self) {
        let subs = self.subscriber_ids(|r| r.caster_account_subscribers());
        if subs.is_empty() {
            return;
        }
        let Some(payload) = self.state.lock().ok().and_then(|s| s.caster_account_payload()) else {
            return;
        };
        self.fanout("casterAccount.update", payload, &subs);
    }

    pub fn broadcast_caster_journal(&self) {
        let subs = self.subscriber_ids(|r| r.caster_journal_subscribers());
        if subs.is_empty() {
            return;
        }
        let Some(payload) = self.state.lock().ok().and_then(|s| s.caster_journal_payload()) else {
            return;
        };
        self.fanout("casterJournal.update", payload, &subs);
    }

    pub fn broadcast_caster_ticket(&self, fill: &Value) {
        if !fill.is_object() {
            return;
        }
        let subs = self.subscriber_ids(|r| r.caster_ticket_subscribers());
        if subs.is_empty() {
            return;
        }
        self.fanout("casterTicket.notify", fill.clone(), &subs);
    }

    pub fn broadcast_caster_blowup(&self, event: &Value) {
        if !event.is_object() {
            return;
        }
        let subs = self.subscriber_ids(|r| r.caster_blowup_subscribers());
        if subs.is_empty() {
            return;
        }
        self.fanout("casterBlowup.notify", event.clone(), &subs);
    }

    pub fn broadcast_admin_status(&self) {
        let (admin_subs, status, payload) = match self.registry.lock() {
            Ok(reg) => {
                let admin_subs: Vec<ConnId> = reg.admin_subscribers().iter().copied().collect();
                let (status, raw) = reg.status_and_admin_payload(&self.host, self.port);
                let payload = self.enrich_admin_payload(raw);
                (admin_subs, status, payload)
            }
            Err(_) => return,
        };
        if let Some(bytes) = encode_line_with_payload("admin.update", payload) {
            for id in admin_subs {
                (self.write_bytes)(id, &bytes);
            }
        }
        (self.notify_status)(status);
    }

    pub fn on_monitor_publisher_disconnect(&self) {
        if let Ok(mut reg) = self.registry.lock() {
            reg.clear_monitor_publisher();
        }
        if let Ok(mut state) = self.state.lock() {
            state.clear_monitor_publisher_state();
        }
        self.broadcast_watchlist();
        self.broadcast_vault();
        self.broadcast_admin_status();
    }
}

fn msg_id(msg: &Value) -> Option<String> {
    msg.get("id").map(|v| match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    })
}

pub fn handle_message(ctx: &HubContext, conn: ConnId, msg: &Value) {
    let Some(obj) = msg.as_object() else {
        ctx.write(
            conn,
            &HubContext::error_line(None, "invalid_envelope", "Expected JSON object"),
        );
        return;
    };

    let schema = obj.get("schema").and_then(|v| v.as_i64());
    let msg_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let id = msg_id(msg);
    let id_ref = id.as_deref().filter(|s| !s.is_empty());
    let payload = obj.get("payload").cloned().unwrap_or(Value::Null);

    if schema != Some(1) {
        ctx.write(
            conn,
            &HubContext::error_line(
                id_ref,
                "unsupported_schema",
                "Only schema 1 is supported",
            ),
        );
        return;
    }

    match msg_type {
        "hello" => {
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "hello.ack",
                    "id": id_ref,
                    "payload": {
                        "app": "arcane-bridge",
                        "protocol": 1,
                        "version": ctx.version
                    }
                }),
            );
        }
        "monitor.register" => {
            if let Ok(mut reg) = ctx.registry.lock() {
                reg.set_monitor_publisher(conn);
                reg.tag(conn, ClientRole::Monitor, id_ref.unwrap_or("monitor-publisher"));
            }
            ctx.broadcast_admin_status();
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "monitor.register.ack",
                    "id": id_ref,
                    "payload": { "ok": true }
                }),
            );
        }
        "session.request" => {
            let token = ctx.state.lock().ok().and_then(|s| s.monitor_token().map(str::to_string));
            if let Some(token) = token {
                ctx.write(
                    conn,
                    &json!({
                        "schema": 1,
                        "type": "session.response",
                        "id": id_ref,
                        "payload": { "ok": true, "token": token }
                    }),
                );
            } else {
                ctx.write(
                    conn,
                    &json!({
                        "schema": 1,
                        "type": "session.response",
                        "id": id_ref,
                        "payload": { "ok": false, "reason": "not_logged_in" }
                    }),
                );
            }
        }
        "session.publish" => {
            if let Ok(mut state) = ctx.state.lock() {
                let token = payload
                    .get("token")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                state.set_monitor_token(token);
            }
        }
        "watchlist.publish" => {
            if let Ok(mut state) = ctx.state.lock() {
                state.merge_watchlist_publish(&payload);
            }
            ctx.broadcast_watchlist();
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "watchlist.publish.ack",
                    "id": id_ref,
                    "payload": { "ok": true }
                }),
            );
        }
        "vault.publish" => {
            if let Ok(mut state) = ctx.state.lock() {
                state.merge_vault_publish(&payload);
            }
            ctx.broadcast_vault();
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "vault.publish.ack",
                    "id": id_ref,
                    "payload": { "ok": true }
                }),
            );
        }
        "feedFocus.publish" => {
            let sym = payload
                .get("symbol")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_uppercase());
            if let Ok(mut state) = ctx.state.lock() {
                state.set_feed_focus_symbol(sym.clone());
            }
            ctx.broadcast_watchlist();
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "feedFocus.ack",
                    "id": id_ref,
                    "payload": { "ok": true, "symbol": sym }
                }),
            );
        }
        "casterAccount.publish" => {
            if let Ok(mut state) = ctx.state.lock() {
                state.merge_caster_account_publish(&payload);
            }
            ctx.broadcast_caster_account();
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "casterAccount.ack",
                    "id": id_ref,
                    "payload": { "ok": true }
                }),
            );
        }
        "casterAccount.subscribe" => {
            if let Ok(mut reg) = ctx.registry.lock() {
                reg.caster_account_subscribers_mut().insert(conn);
            }
            if let Some(snapshot) = ctx.state.lock().ok().and_then(|s| s.caster_account_payload()) {
                ctx.write(
                    conn,
                    &json!({
                        "schema": 1,
                        "type": "casterAccount.snapshot",
                        "id": id_ref,
                        "payload": snapshot
                    }),
                );
            }
        }
        "casterAccount.request" => {
            let payload = ctx
                .state
                .lock()
                .ok()
                .and_then(|s| s.caster_account_payload())
                .unwrap_or(json!({}));
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "casterAccount.response",
                    "id": id_ref,
                    "payload": payload
                }),
            );
        }
        "casterJournal.publish" => {
            if let Ok(mut state) = ctx.state.lock() {
                state.merge_caster_journal_publish(&payload);
            }
            ctx.broadcast_caster_journal();
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "casterJournal.ack",
                    "id": id_ref,
                    "payload": { "ok": true }
                }),
            );
        }
        "casterJournal.subscribe" => {
            if let Ok(mut reg) = ctx.registry.lock() {
                reg.caster_journal_subscribers_mut().insert(conn);
            }
            if let Some(snapshot) = ctx.state.lock().ok().and_then(|s| s.caster_journal_payload()) {
                ctx.write(
                    conn,
                    &json!({
                        "schema": 1,
                        "type": "casterJournal.snapshot",
                        "id": id_ref,
                        "payload": snapshot
                    }),
                );
            }
        }
        "casterJournal.request" => {
            let payload = ctx
                .state
                .lock()
                .ok()
                .and_then(|s| s.caster_journal_payload())
                .unwrap_or(json!({}));
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "casterJournal.response",
                    "id": id_ref,
                    "payload": payload
                }),
            );
        }
        "casterTicket.notify" => {
            if payload.is_object() {
                ctx.broadcast_caster_ticket(&payload);
            }
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "casterTicket.ack",
                    "id": id_ref,
                    "payload": { "ok": true }
                }),
            );
        }
        "casterTicket.subscribe" => {
            if let Ok(mut reg) = ctx.registry.lock() {
                reg.caster_ticket_subscribers_mut().insert(conn);
            }
        }
        "casterBlowup.notify" => {
            if payload.is_object() {
                ctx.broadcast_caster_blowup(&payload);
            }
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "casterBlowup.ack",
                    "id": id_ref,
                    "payload": { "ok": true }
                }),
            );
        }
        "casterBlowup.subscribe" => {
            if let Ok(mut reg) = ctx.registry.lock() {
                reg.caster_blowup_subscribers_mut().insert(conn);
            }
        }
        "guildsFeed.publish" => {
            if let Ok(mut state) = ctx.state.lock() {
                state.merge_guilds_feed_publish(&payload);
            }
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "guildsFeed.ack",
                    "id": id_ref,
                    "payload": { "ok": true }
                }),
            );
        }
        "guildsFeed.request" => {
            let payload = ctx
                .state
                .lock()
                .ok()
                .and_then(|s| s.guilds_feed_payload())
                .unwrap_or(json!({}));
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "guildsFeed.response",
                    "id": id_ref,
                    "payload": payload
                }),
            );
        }
        "watchlist.subscribe" => {
            let role = infer_role_from_client_id(id_ref);
            if let Ok(mut reg) = ctx.registry.lock() {
                reg.watchlist_subscribers_mut().insert(conn);
                reg.tag(conn, role, id_ref.unwrap_or("watchlist-subscriber"));
            }
            ctx.broadcast_admin_status();
            let snapshot = ctx
                .state
                .lock()
                .map(|s| s.watchlist_payload())
                .unwrap_or(json!({}));
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "watchlist.snapshot",
                    "id": id_ref,
                    "payload": snapshot
                }),
            );
        }
        "admin.subscribe" => {
            let snapshot = if let Ok(mut reg) = ctx.registry.lock() {
                reg.admin_subscribers_mut().insert(conn);
                reg.tag(conn, ClientRole::BridgeApp, id_ref.unwrap_or("bridge-admin"));
                reg.admin_payload(&ctx.host, ctx.port)
            } else {
                json!({})
            };
            let snapshot = ctx.enrich_admin_payload(snapshot);
            ctx.broadcast_admin_status();
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "admin.snapshot",
                    "id": id_ref,
                    "payload": snapshot
                }),
            );
        }
        "admin.request" => {
            let snapshot = ctx
                .registry
                .lock()
                .map(|r| r.admin_payload(&ctx.host, ctx.port))
                .unwrap_or(json!({}));
            let snapshot = ctx.enrich_admin_payload(snapshot);
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "admin.response",
                    "id": id_ref,
                    "payload": snapshot
                }),
            );
        }
        "watchlist.request" => {
            let snapshot = ctx
                .state
                .lock()
                .map(|s| s.watchlist_payload())
                .unwrap_or(json!({}));
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "watchlist.response",
                    "id": id_ref,
                    "payload": snapshot
                }),
            );
        }
        "vault.subscribe" => {
            if let Ok(mut reg) = ctx.registry.lock() {
                reg.vault_subscribers_mut().insert(conn);
            }
            let snapshot = ctx
                .state
                .lock()
                .map(|s| s.vault_payload())
                .unwrap_or(json!({}));
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "vault.snapshot",
                    "id": id_ref,
                    "payload": snapshot
                }),
            );
        }
        "vault.request" => {
            let snapshot = ctx
                .state
                .lock()
                .map(|s| s.vault_payload())
                .unwrap_or(json!({}));
            ctx.write(
                conn,
                &json!({
                    "schema": 1,
                    "type": "vault.response",
                    "id": id_ref,
                    "payload": snapshot
                }),
            );
        }
        _ => {
            ctx.write(
                conn,
                &HubContext::error_line(
                    id_ref,
                    "unknown_type",
                    &format!("Unknown type: {msg_type}"),
                ),
            );
        }
    }
}

const FORWARD_TO_MONITOR: &[&str] = &[
    "watchlist.add",
    "watchlist.remove",
    "activeHero.toggle",
    "activeHero.setEnabled",
];

const MONITOR_PUBLISH_TYPES: &[&str] = &[
    "monitor.register",
    "session.publish",
    "watchlist.publish",
    "vault.publish",
];

pub fn is_forward_to_monitor(msg_type: &str) -> bool {
    FORWARD_TO_MONITOR.contains(&msg_type)
}

pub fn is_monitor_publish_type(msg_type: &str) -> bool {
    MONITOR_PUBLISH_TYPES.contains(&msg_type)
}

pub fn handle_monitor_publisher_line(ctx: &HubContext, _publisher: ConnId, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
        return;
    };

    let req_id = msg_id(&msg);
    if let Some(ref rid) = req_id {
        if let Ok(mut pending) = ctx.pending_by_request_id.lock() {
            if let Some(client) = pending.remove(rid) {
                ctx.write_raw_line(client, trimmed);
                if msg.get("type").and_then(|v| v.as_str()) == Some("watchlist.response") {
                    if let Some(payload) = msg.get("payload") {
                        if let Ok(mut state) = ctx.state.lock() {
                            state.merge_watchlist_publish(payload);
                        }
                        ctx.broadcast_watchlist();
                    }
                }
                return;
            }
        }
    }

    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if is_monitor_publish_type(msg_type) {
        handle_message(ctx, _publisher, &msg);
    }
}

pub fn handle_client_line(ctx: &HubContext, conn: ConnId, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
        return;
    };

    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let id = msg_id(&msg);

    if is_forward_to_monitor(msg_type) {
        let publisher = ctx.registry.lock().ok().and_then(|r| r.monitor_publisher());
        let Some(publisher) = publisher else {
            ctx.write(
                conn,
                &HubContext::error_line(
                    id.as_deref(),
                    "monitor_unavailable",
                    "Arcane Monitor is not connected",
                ),
            );
            return;
        };
        if let Some(ref rid) = id {
            if let Ok(mut pending) = ctx.pending_by_request_id.lock() {
                pending.insert(rid.clone(), conn);
            }
        }
        ctx.write_raw_line(publisher, trimmed);
        return;
    }

    handle_message(ctx, conn, &msg);
}
