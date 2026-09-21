use anyhow::{Context, Result};
use serde_json::{json, Value};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";

async fn call(api_key: &str, state: Value, questions: Value) -> Result<Value> {
    let client = reqwest::Client::new();
    let body = json!({ "model": "jev-latest", "state": state, "questions": questions });
    let resp = client
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .context("POST systemone")?;
    let status = resp.status();
    let v: Value = resp.json().await.context("decode systemone")?;
    if !status.is_success() {
        anyhow::bail!("jev {}: {}", status, v);
    }
    Ok(v)
}

/// Minimal liveness check: single Noul question. Returns raw Jev JSON.
pub async fn check(api_key: &str) -> Result<Value> {
    call(
        api_key,
        json!({"probe": "porter liveness"}),
        json!({"ok": {"type": "noul", "instructions": "Is this a liveness probe?"}}),
    )
    .await
}

/// Route decision: Choice over LLM providers + Noul escalation flag, one round trip.
pub async fn route(api_key: &str, agent: &str, task: &str) -> Result<Value> {
    call(
        api_key,
        json!({"agent": agent, "task": task}),
        json!({
            "route_llm": {
                "type": "choice",
                "instructions": "Which LLM provider should handle this task?",
                "criteria": {
                    "meta": "Fast local-ish chat, muse-spark default",
                    "deepseek": "Reasoning-heavy code/math task",
                    "openrouter": "Fallback or exotic model needed"
                }
            },
            "needs_escalation": {
                "type": "noul",
                "instructions": "Does this task need human confirmation before acting?"
            }
        }),
    )
    .await
}
