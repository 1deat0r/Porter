//! Cloudflare: token check via the standard verify endpoint.

use serde_json::{json, Value};

/// Key test: GET /client/v4/user/tokens/verify.
pub fn check(agent: &str) -> Result<Value, String> {
    let key = crate::vault::get(agent, "cloudflare").ok_or_else(|| "no cloudflare key".to_string())?;
    let v: Value = reqwest::blocking::Client::new()
        .get("https://api.cloudflare.com/client/v4/user/tokens/verify")
        .bearer_auth(&key)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .unwrap_or(Value::Null);
    match v.get("success").and_then(|s| s.as_bool()) {
        Some(true) => Ok(json!({"ok": true})),
        _ => Err(format!("cloudflare: token invalid ({v})")),
    }
}
