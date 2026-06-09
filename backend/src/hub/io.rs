//! JSONL write helpers — serialize once per broadcast fan-out.

use serde_json::Value;
use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

pub fn write_bytes_to_stream(stream: &Arc<Mutex<TcpStream>>, bytes: &[u8]) {
    if let Ok(mut s) = stream.lock() {
        let _ = s.write_all(bytes);
        let _ = s.flush();
    }
}

pub fn encode_json_line(obj: &Value) -> Option<Vec<u8>> {
    let mut line = serde_json::to_string(obj).ok()?;
    line.push('\n');
    Some(line.into_bytes())
}

pub fn encode_line_with_payload(msg_type: &str, payload: Value) -> Option<Vec<u8>> {
    encode_json_line(&serde_json::json!({
        "schema": 1,
        "type": msg_type,
        "id": Value::Null,
        "payload": payload,
    }))
}
