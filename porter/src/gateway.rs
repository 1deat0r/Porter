//! OpenAI-dialect gateway: harnesses point base_url at porter.
//! Auth: porter bearer tokens (`porter token issue <agent>`).
//! Models: porter/<provider>/<model> | porter/auto (Jev picks provider).

use axum::{
    body::Body,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures_util::StreamExt;
use serde_json::{json, Value};

pub const ROUTABLE: &[&str] = &["deepseek", "openrouter", "openai", "meta", "gemini", "anthropic"];

const AUTO_DEFAULTS: &[(&str, &str)] = &[
    ("deepseek", "deepseek-chat"),
    ("openai", "gpt-4o-mini"),
    ("anthropic", "claude-3-5-haiku-latest"),
    ("gemini", "gemini-2.0-flash"),
];

fn openai_err(msg: String) -> Json<Value> {
    Json(json!({"error": {"message": msg, "type": "invalid_request_error"}}))
}

type Failure = (StatusCode, Json<Value>);

fn provider_base(agent: &str, provider: &str) -> String {
    if provider == "anthropic" {
        return "https://api.anthropic.com".into();
    }
    let ep = crate::vault::get_endpoint(agent, provider);
    let ep = ep.trim().trim_end_matches('/').to_string();
    if ep.is_empty() {
        "https://api.invalid".into()
    } else {
        ep
    }
}

/// Bearer token -> agent scope. No token, no spend.
pub fn authenticate(headers: &HeaderMap) -> Result<String, Failure> {
    let tok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim())
        .unwrap_or("");
    match crate::vault::find_token_agent(tok) {
        Some(a) if !tok.is_empty() => Ok(a),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            openai_err("invalid porter token — issue one with `porter token issue <agent>`".into()),
        )),
    }
}

pub enum Route {
    Direct { provider: String, model: String },
    Auto,
}

pub fn parse_model(model: &str) -> Result<Route, String> {
    let m = model.trim();
    if m == "porter/auto" {
        return Ok(Route::Auto);
    }
    if let Some(rest) = m.strip_prefix("porter/") {
        if let Some((p, target)) = rest.split_once('/') {
            if ROUTABLE.contains(&p) && !target.is_empty() {
                return Ok(Route::Direct { provider: p.into(), model: target.into() });
            }
        }
    }
    Err("unknown model — use porter/<provider>/<model> or porter/auto".into())
}

fn last_user_text(body: &Value) -> String {
    body.get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .rev()
                .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                .and_then(|m| m.get("content").and_then(|c| c.as_str()))
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default()
}

async fn resolve_auto(agent: &str, task: &str) -> Result<(String, String), String> {
    let key = crate::vault::get(agent, "typesafe").ok_or_else(|| "auto needs a jev key".to_string())?;
    let v = crate::jev::route(&key, agent, task).await.map_err(|e| e.to_string())?;
    let mut ranked: Vec<(String, f64)> = v
        .pointer("/answers/route_llm/probabilities")
        .and_then(|p| p.as_object())
        .map(|o| o.iter().filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f))).collect())
        .unwrap_or_default();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (p, _) in &ranked {
        if let Some((_, m)) = AUTO_DEFAULTS.iter().find(|(d, _)| d == p) {
            if crate::vault::has(agent, p) {
                return Ok((p.clone(), m.to_string()));
            }
        }
    }
    // Jev shape unexpected or picks unkeyed: first keyed default wins.
    for (p, m) in AUTO_DEFAULTS {
        if crate::vault::has(agent, p) {
            return Ok((p.to_string(), m.to_string()));
        }
    }
    Err("auto found no keyed provider — paste a key or use porter/<provider>/<model>".into())
}

/// Model list in OpenAI shape. Only keyed providers appear.
pub fn models(agent: &str) -> Value {
    let mut data = vec![json!({"id": "porter/auto", "object": "model", "owned_by": "porter"})];
    for (p, m) in [
        ("deepseek", "deepseek-chat"),
        ("deepseek", "deepseek-reasoner"),
        ("openai", "gpt-4o-mini"),
        ("anthropic", "claude-3-5-haiku-latest"),
        ("gemini", "gemini-2.0-flash"),
    ] {
        if crate::vault::has(agent, p) {
            data.push(json!({"id": format!("porter/{p}/{m}"), "object": "model", "owned_by": "porter"}));
        }
    }
    if crate::vault::has(agent, "openrouter") {
        data.push(json!({"id": "porter/openrouter/<model-id>", "object": "model", "owned_by": "porter"}));
    }
    json!({"object": "list", "data": data})
}

pub async fn chat(agent: &str, body: &Value) -> Result<Response, Failure> {
    let model = body
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or((StatusCode::BAD_REQUEST, openai_err("no model".into())))?;
    let (provider, target) = match parse_model(model).map_err(|e| (StatusCode::BAD_REQUEST, openai_err(e)))? {
        Route::Direct { provider, model } => (provider, model),
        Route::Auto => resolve_auto(agent, &last_user_text(body))
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, openai_err(e)))?,
    };
    let key = crate::vault::get(agent, &provider).ok_or((
        StatusCode::UNAUTHORIZED,
        openai_err(format!("no {provider} key")),
    ))?;
    let base = provider_base(agent, &provider);
    let (url, pbody) = if provider == "anthropic" {
        let max = body.get("max_tokens").cloned().unwrap_or(json!(1024));
        (
            format!("{base}/v1/messages"),
            json!({"model": target, "max_tokens": max, "messages": body.get("messages").cloned().unwrap_or(json!([]))}),
        )
    } else {
        let mut b = body.clone();
        b["model"] = json!(target);
        (format!("{base}/chat/completions"), b)
    };
    let client = reqwest::Client::new();
    let mut req = client.post(&url);
    req = if provider == "anthropic" {
        req.header("x-api-key", &key).header("anthropic-version", "2023-06-01")
    } else {
        req.bearer_auth(&key)
    };
    let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    let resp = req
        .json(&pbody)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, openai_err(e.to_string())))?;
    let status = resp.status();
    if !status.is_success() {
        let code = status.as_u16();
        let text = resp.text().await.unwrap_or_default();
        let text: String = text.chars().take(300).collect();
        return Err((
            StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(json!({"error": {"message": format!("{provider}: {text}"), "type": "provider_error"}})),
        ));
    }
    if stream {
        let s = resp.bytes_stream().map(|c| c.map_err(anyhow::Error::from));
        Ok(Response::builder()
            .header("content-type", "text/event-stream")
            .body(Body::from_stream(s))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, openai_err(e.to_string())))?
            .into_response())
    } else {
        let v: Value = resp
            .json()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, openai_err(e.to_string())))?;
        Ok(Json(v).into_response())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_shapes() {
        match parse_model("porter/deepseek/deepseek-chat").unwrap() {
            Route::Direct { provider, model } => {
                assert_eq!(provider, "deepseek");
                assert_eq!(model, "deepseek-chat");
            }
            Route::Auto => panic!(),
        }
        match parse_model("porter/openrouter/x/y").unwrap() {
            Route::Direct { provider, model } => {
                assert_eq!(provider, "openrouter");
                assert_eq!(model, "x/y");
            }
            Route::Auto => panic!(),
        }
        assert!(matches!(parse_model("porter/auto").unwrap(), Route::Auto));
        assert!(parse_model("gpt-4").is_err());
        assert!(parse_model("porter/notion/x").is_err());
    }

    #[test]
    fn token_roundtrip() {
        let dir = std::env::temp_dir().join(format!("porter-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let prev = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let tok = crate::vault::issue_token("codex").unwrap();
        assert!(tok.starts_with("ptr_"));
        assert_eq!(crate::vault::find_token_agent(&tok).as_deref(), Some("codex"));
        assert!(crate::vault::find_token_agent("ptr_deadbeef").is_none());
        let listed = crate::vault::list_tokens();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "codex");
        assert!(crate::vault::revoke_token("codex", &listed[0].1).unwrap());
        assert!(crate::vault::find_token_agent(&tok).is_none());
        match prev {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
