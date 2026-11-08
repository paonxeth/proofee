// События NEP-297 (SPEC 8).
use near_sdk::env;
use near_sdk::serde_json::{Value, json};

pub fn emit(event: &str, data: Value) {
    let log = json!({
        "standard": "proofee",
        "version": "1.0.0",
        "event": event,
        "data": [data],
    });
    env::log_str(&format!("EVENT_JSON:{log}"));
}
