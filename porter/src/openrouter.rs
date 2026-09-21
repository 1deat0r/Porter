//! OpenRouter provider: OpenAI-compatible chat + model list.
//! Key test is GET /models. Chat posts /chat/completions.

use serde_json::{json, Value};

const DEFAULT_BASE: &str = "https://openrouter.ai/api/v1";

fn base_for(agent: &str) -> String {
    let ep = crate::vault::get_endpoint(agent, "openrouter");
    let ep = ep.trim().trim_end_matches('/').to_string();
    if ep.is_empty() {
        DEFAULT_BASE.into()
    } else {
        ep
    }
}

fn key_for(agent: &str) -> Result<String, String> {
    crate::vault::get(agent, "openrouter").ok_or_else(|| "no openrouter key".to_string())
}

fn map_error(status: u16, v: &Value) -> String {
    let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
    let inner = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("");
    let detail = if inner.is_empty() { msg } else { inner };
    match status {
        401 => "unauthorized: bad key — repaste it".into(),
        402 => "no credits: add funds at openrouter.ai/credits".into(),
        429 => "rate limited: back off and retry".into(),
        _ => format!("openrouter {status}: {detail}"),
    }
}

fn send(path: &str, agent: &str, body: Option<Value>) -> Result<Value, String> {
    let key = key_for(agent)?;
    let url = format!("{}{}", base_for(agent), path);
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
        Err(map_error(status, &v))
    }
}

/// Key test: list models visible to this key.
pub fn check(agent: &str) -> Result<Value, String> {
    send("/models", agent, None)
}

pub fn chat_body(model: &str, messages: &Value, max_tokens: Option<u32>) -> Value {
    let mut b = json!({"model": model, "messages": messages});
    if let Some(n) = max_tokens {
        b["max_tokens"] = json!(n);
    }
    b
}

/// One chat completion. `messages` is the OpenAI-format array.
pub fn chat(agent: &str, model: &str, messages: &Value, max_tokens: Option<u32>) -> Result<Value, String> {
    send("/chat/completions", agent, Some(chat_body(model, messages, max_tokens)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_shape() {
        let msgs = json!([{"role": "user", "content": "hi"}]);
        let b = chat_body("x/y", &msgs, Some(50));
        assert_eq!(b["model"], "x/y");
        assert_eq!(b["messages"], msgs);
        assert_eq!(b["max_tokens"], 50);
    }

    #[test]
    fn body_no_cap() {
        let b = chat_body("x/y", &json!([]), None);
        assert!(b.get("max_tokens").is_none());
    }
}
