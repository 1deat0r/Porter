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

fn backup_existing() {
    let p = vault_path();
    if !p.exists() {
        return;
    }
    let dir = p.parent().map(|x| x.join("backups"));
    let Some(dir) = dir else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest = dir.join(format!("vault-{ts}.json"));
    if std::fs::copy(&p, &dest).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
        }
        if let Ok(list) = std::fs::read_dir(&dir) {
            let mut files: Vec<_> = list.filter_map(|e| e.ok().map(|x| x.path())).collect();
            files.sort();
            let drop = files.len().saturating_sub(10);
            for old in files.into_iter().take(drop) {
                let _ = std::fs::remove_file(old);
            }
        }
    }
}

fn save_entries(entries: &HashMap<String, String>) -> Result<()> {
    let p = vault_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).context("mkdir vault dir")?;
    }
    backup_existing();
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

fn kr_get(user: &str) -> Option<String> {
    keyring::Entry::new("porter", user)
        .ok()?
        .get_password()
        .ok()
        .filter(|v| !v.is_empty())
}

fn kr_set(user: &str, value: &str) {
    if let Ok(e) = keyring::Entry::new("porter", user) {
        let _ = e.set_password(value);
    }
}

fn kr_delete(user: &str) {
    if let Ok(e) = keyring::Entry::new("porter", user) {
        let _ = e.delete_credential();
    }
}

fn file_hit(entries: &HashMap<String, String>, id: &str) -> Option<String> {
    entries.get(id).filter(|v| !v.is_empty()).cloned()
}

/// Lookup: OS keyring agent -> keyring global -> file agent -> file global
/// (lazy-migrated into the keyring) -> env fallbacks -> TYPESAFE for jev.
pub fn get(agent: &str, provider: &str) -> Option<String> {
    let id = key_id(agent, provider);
    if let Some(v) = kr_get(&id) {
        return Some(v);
    }
    let gid = key_id("global", provider);
    if let Some(v) = kr_get(&gid) {
        return Some(v);
    }
    let entries = file_entries();
    if let Some(v) = file_hit(&entries, &id) {
        kr_set(&id, &v);
        return Some(v);
    }
    if let Some(v) = file_hit(&entries, &gid) {
        kr_set(&gid, &v);
        return Some(v);
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
    let id = key_id(agent, provider);
    kr_set(&id, value);
    let mut entries = file_entries();
    entries.insert(id, value.to_string());
    save_entries(&entries)
}

pub fn remove_id(id: &str) -> Result<()> {
    kr_delete(id);
    let mut entries = file_entries();
    entries.remove(id);
    save_entries(&entries)
}

/// Delete a stored secret from keyring and file. Env fallbacks still apply.
pub fn clear(agent: &str, provider: &str) -> Result<bool> {
    let existed = has(agent, provider);
    remove_id(&key_id(agent, provider))?;
    Ok(existed)
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

#[cfg(test)]
mod tests {
    #[test]
    fn keyring_roundtrip() {
        let e = keyring::Entry::new("porter-test", "roundtrip").expect("entry");
        e.set_password("v").expect("set");
        assert_eq!(e.get_password().expect("get"), "v");
        e.delete_credential().expect("del");
    }
}

fn rand_hex(nbytes: usize) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open("/dev/urandom").map_err(|e| e.to_string())?;
    let mut b = vec![0u8; nbytes];
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn sha_hex(s: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

/// Issue a porter bearer token for an agent scope. Returned once; only the
/// sha256 is stored (never the token). Format: ptr_<48 hex>.
pub fn issue_token(agent: &str) -> Result<String, String> {
    let agent = agent.trim().to_lowercase();
    let tok = format!("ptr_{}", rand_hex(24).map_err(|e| e.to_string())?);
    let prefix: String = tok.chars().skip(4).take(8).collect();
    let mut entries = file_entries();
    entries.insert(format!("token/{agent}/{prefix}"), sha_hex(&tok));
    save_entries(&entries).map_err(|e| e.to_string())?;
    Ok(tok)
}

/// Resolve a presented bearer token to its agent scope.
pub fn find_token_agent(presented: &str) -> Option<String> {
    let want = sha_hex(presented.trim());
    file_entries().iter().find_map(|(k, v)| {
        if k.starts_with("token/") && v == &want {
            k.split('/').nth(1).map(|s| s.to_string())
        } else {
            None
        }
    })
}

/// (agent, prefix) for every issued token. Hashes stay hidden.
pub fn list_tokens() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = file_entries()
        .keys()
        .filter_map(|k| {
            let mut parts = k.split('/');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("token"), Some(a), Some(p)) => Some((a.to_string(), p.to_string())),
                _ => None,
            }
        })
        .collect();
    out.sort();
    out
}

pub fn revoke_token(agent: &str, prefix: &str) -> Result<bool, String> {
    let id = format!("token/{}/{}", agent.trim().to_lowercase(), prefix.trim());
    let mut entries = file_entries();
    let gone = entries.remove(&id).is_some();
    save_entries(&entries).map_err(|e| e.to_string())?;
    Ok(gone)
}
