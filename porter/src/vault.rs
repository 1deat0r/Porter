use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

pub fn vault_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME").ok().filter(|v| !v.trim().is_empty()).ok_or(std::env::VarError::NotPresent)
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".config")
        });
    base.join("porter").join("vault.json")
}

fn file_entries() -> HashMap<String, String> {
    let p = vault_path();
    if let Ok(bytes) = std::fs::read(&p) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(obj) = v.get("entries").and_then(|e| e.as_object()) {
                return obj
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
            }
        }
    }
    HashMap::new()
}

fn save_entries(entries: &HashMap<String, String>) -> Result<()> {
    let p = vault_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).context("mkdir vault dir")?;
    }
    let v = serde_json::json!({ "entries": entries });
    std::fs::write(&p, serde_json::to_string_pretty(&v)?).context("write vault")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn key_id(agent: &str, provider: &str) -> String {
    format!("{}/{}", agent.trim().to_lowercase(), provider.trim().to_lowercase())
}

fn ep_id(scope: &str, provider: &str) -> String {
    format!("{}/{}_endpoint", scope.trim().to_lowercase(), provider.trim().to_lowercase())
}

/// Lookup: vault agent/provider -> vault global/provider -> env fallbacks -> TYPESAFE for jev.
pub fn get(agent: &str, provider: &str) -> Option<String> {
    let entries = file_entries();
    if let Some(v) = entries.get(&key_id(agent, provider)) {
        if !v.is_empty() {
            return Some(v.clone());
        }
    }
    if let Some(v) = entries.get(&key_id("global", provider)) {
        if !v.is_empty() {
            return Some(v.clone());
        }
    }
    let a = agent.to_uppercase().replace('-', "_");
    let p = provider.to_uppercase().replace('-', "_");
    for cand in [
        format!("{a}_{p}"),
        format!("{a}_{p}_API_KEY"),
        format!("{p}_API_KEY"),
        format!("{p}"),
    ] {
        if let Ok(v) = std::env::var(&cand) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    if provider.eq_ignore_ascii_case("typesafe") || provider.eq_ignore_ascii_case("jev") {
        if let Ok(v) = std::env::var("TYPESAFE_API_KEY") {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

pub fn set(agent: &str, provider: &str, value: &str) -> Result<()> {
    let mut entries = file_entries();
    entries.insert(key_id(agent, provider), value.to_string());
    save_entries(&entries)
}

pub fn remove_id(id: &str) -> Result<()> {
    let mut entries = file_entries();
    entries.remove(id);
    save_entries(&entries)
}

pub fn list_names() -> Vec<String> {
    let mut out: Vec<String> = file_entries().keys().cloned().collect();
    out.sort();
    out
}

pub fn has(agent: &str, provider: &str) -> bool {
    get(agent, provider).is_some()
}

pub fn default_endpoint(provider: &str) -> &'static str {
    match provider.to_lowercase().as_str() {
        "typesafe" | "jev" => "https://api.typesafe.ai/v1/systemone",
        "meta" => "https://api.meta.ai/v1",
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        "deepseek" => "https://api.deepseek.com/v1",
        "gemini" => "https://generativelanguage.googleapis.com/v1beta",
        "notion" => "https://api.notion.com/v1",
        "supabase" => "",
        "cloudflare" => "https://api.cloudflare.com/client/v4",
        "minara" => "",
        _ => "",
    }
}

/// Effective endpoint: scope-specific custom -> global custom -> built-in default.
pub fn get_endpoint(scope: &str, provider: &str) -> String {
    let entries = file_entries();
    if !scope.eq_ignore_ascii_case("global") {
        if let Some(v) = entries.get(&ep_id(scope, provider)) {
            return v.clone();
        }
    }
    if let Some(v) = entries.get(&ep_id("global", provider)) {
        return v.clone();
    }
    default_endpoint(provider).to_string()
}

pub fn endpoint_is_custom(scope: &str, provider: &str) -> bool {
    let entries = file_entries();
    if !scope.eq_ignore_ascii_case("global") && entries.contains_key(&ep_id(scope, provider)) {
        return true;
    }
    entries.contains_key(&ep_id("global", provider))
}

/// Set custom endpoint; empty string clears back to default.
pub fn set_endpoint(scope: &str, provider: &str, url: &str) -> Result<()> {
    let url = url.trim();
    if url.is_empty() {
        let id = ep_id(scope, provider);
        remove_id(&id)
    } else {
        let mut entries = file_entries();
        entries.insert(ep_id(scope, provider), url.to_string());
        save_entries(&entries)
    }
}
