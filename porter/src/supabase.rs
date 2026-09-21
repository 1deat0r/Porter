//! Supabase: key check via Auth user endpoint + read-only REST GET passthrough.
//! Needs a custom endpoint first (https://xyz.supabase.co) — no shared default.

use serde_json::Value;

fn base_for(agent: &str) -> Result<String, String> {
    let ep = crate::vault::get_endpoint(agent, "supabase");
    let ep = ep.trim().trim_end_matches('/').to_string();
    if ep.is_empty() {
        Err("no supabase endpoint — set it in Porter Connectors first".into())
    } else {
        Ok(ep)
    }
}

fn key_for(agent: &str) -> Result<String, String> {
    crate::vault::get(agent, "supabase").ok_or_else(|| "no supabase key".to_string())
}

fn authed(path: &str, agent: &str) -> Result<Value, String> {
    let key = key_for(agent)?;
    let url = format!("{}{}", base_for(agent)?, path);
    let v: Value = reqwest::blocking::Client::new()
        .get(&url)
        .header("apikey", &key)
        .bearer_auth(&key)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .unwrap_or(Value::Null);
    Ok(v)
}

/// Key test: succeeds (object, not an error with message) when the key is valid.
pub fn check(agent: &str) -> Result<Value, String> {
    let v = authed("/auth/v1/user", agent)?;
    if v.get("msg").is_some() || v.get("message").is_some() {
        Err(format!("supabase: invalid key or endpoint ({v})"))
    } else {
        Ok(v)
    }
}

/// Read-only GET against /rest/v1/<path>, e.g. "todos?select=*".
pub fn rest_get(agent: &str, path: &str) -> Result<Value, String> {
    let clean = path.trim().trim_start_matches('/');
    if clean.is_empty() {
        return Err("empty rest path".into());
    }
    authed(&format!("/rest/v1/{clean}"), agent)
}
