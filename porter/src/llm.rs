//! OpenAI-dialect LLM providers (openai, meta, gemini) + Anthropic adapter.
//! Valid providers for the /v1/llm/* routes: openai, meta, gemini, anthropic.

use serde_json::{json, Value};

pub const PROVIDERS: &[&str] = &["openai", "meta", "gemini", "anthropic"];

fn default_base(provider: &str) -> &'static str {
    match provider {
        "openai" => "https://api.openai.com/v1",
        "meta" => "https://api.meta.ai/v1",
        "gemini" => "https://generativelanguage.googleapis.com/v1beta/openai",
        _ => "",
    }
}

fn base_for(agent: &str, provider: &str) -> String {
    if provider == "anthropic" {
        return "https://api.anthropic.com".into();
    }
    let ep = crate::vault::get_endpoint(agent, provider);
    let ep = ep.trim().trim_end_matches('/').to_string();
    if ep.is_empty() {
        default_base(provider).into()
    } else {
        ep
    }
}

fn key_for(agent: &str, provider: &str) -> Result<String, String> {
    crate::vault::get(agent, provider).ok_or_else(|| format!("no {provider} key"))
}

fn send_openai(path: &str, agent: &str, provider: &str, body: Option<Value>) -> Result<Value, String> {
    let key = key_for(agent, provider)?;
    let url = format!("{}{}", base_for(agent, provider), path);
    let c = reqwest::blocking::Client::new();
    let mut r = if body.is_some() { c.post(&url) } else { c.get(&url) };
    r = r.bearer_auth(&key);
    if let Some(b) = body {
        r = r.json(&b);
    }
    let resp = r.send().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let v: Value = resp.json().unwrap_or(Value::Null);
    if (200..300).contains(&status) {
        Ok(v)
    } else {
        let detail = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("");
        Err(match status {
            401 => format!("{provider}: unauthorized — bad key, repaste it"),
            429 => format!("{provider}: rate limited — back off and retry"),
            _ => format!("{provider} {status}: {detail}"),
        })
    }
}

fn send_anthropic(path: &str, agent: &str, body: Option<Value>) -> Result<Value, String> {
    let key = key_for(agent, "anthropic")?;
    let url = format!("https://api.anthropic.com{path}");
    let c = reqwest::blocking::Client::new();
    let mut r = if body.is_some() { c.post(&url) } else { c.get(&url) };
    r = r.header("x-api-key", &key).header("anthropic-version", "2023-06-01");
    if let Some(b) = body {
        r = r.json(&b);
    }
    let resp = r.send().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let v: Value = resp.json().unwrap_or(Value::Null);
    if (200..300).contains(&status) {
        Ok(v)
    } else {
        let detail = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("");
        Err(match status {
            401 => "anthropic: unauthorized — bad key, repaste it".into(),
            429 => "anthropic: rate limited — back off and retry".into(),
            _ => format!("anthropic {status}: {detail}"),
        })
    }
}

/// Key test. OpenAI-dialect: GET /models (count). Anthropic: GET /v1/models.
pub fn check(agent: &str, provider: &str) -> Result<Value, String> {
    if provider == "anthropic" {
        send_anthropic("/v1/models", agent, None)
    } else {
        send_openai("/models", agent, provider, None)
    }
}

pub fn chat_body(model: &str, messages: &Value, max_tokens: Option<u32>) -> Value {
    let mut b = json!({"model": model, "messages": messages});
    if let Some(n) = max_tokens {
        b["max_tokens"] = json!(n);
    }
    b
}

/// OpenAI-dialect chat. Anthropic note: `system` role inside messages is
/// rejected by Anthropic — pass system text via a top-level "system" key
/// through the messages array is NOT supported here; keep to roles user/assistant.
pub fn chat(agent: &str, provider: &str, model: &str, messages: &Value, max_tokens: Option<u32>) -> Result<Value, String> {
    if provider == "anthropic" {
        let mut b = json!({"model": model, "messages": messages, "max_tokens": max_tokens.unwrap_or(1024)});
        let _ = &mut b;
        send_anthropic("/v1/messages", agent, Some(b))
    } else {
        send_openai("/chat/completions", agent, provider, Some(chat_body(model, messages, max_tokens)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_providers() {
        assert!(PROVIDERS.contains(&"openai"));
        assert!(PROVIDERS.contains(&"anthropic"));
        assert!(!PROVIDERS.contains(&"notion"));
    }

    #[test]
    fn body_shape() {
        let b = chat_body("m", &json!([{"role": "user", "content": "hi"}]), Some(5));
        assert_eq!(b["model"], "m");
        assert_eq!(b["max_tokens"], 5);
    }
}
