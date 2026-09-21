use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub default_agent: String,
}

fn agent_from(headers: &HeaderMap, fallback: &str) -> String {
    headers
        .get("x-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(fallback)
        .to_lowercase()
}

pub fn router(default_agent: String) -> Router {
    let st = Arc::new(AppState { default_agent });
    Router::new()
        .route("/health", get(health))
        .route("/settings", get(settings))
        .route("/v1/jev/check", post(jev_check))
        .route("/v1/route/decide", post(route_decide))
        .route("/v1/providers", get(providers))
        .route("/v1/vault/status", get(vault_status))
        .route("/v1/vault/set", post(vault_set))
        .route("/v1/notion/check", post(notion_check))
        .route("/v1/notion/search", post(notion_search))
        .route("/v1/notion/pages", post(notion_pages))
        .route("/v1/openrouter/check", post(openrouter_check))
        .route("/v1/openrouter/chat", post(openrouter_chat))
        .route("/v1/deepseek/check", post(deepseek_check))
        .route("/v1/deepseek/chat", post(deepseek_chat))
        .with_state(st)
}

async fn health() -> Json<Value> {
    Json(json!({"ok": true, "service": "porter"}))
}

async fn settings() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(crate::ui::PAGE),
    )
}

async fn providers() -> Json<Value> {
    Json(json!({
        "agents": ["codex", "hermes", "omp"],
        "jev": "typesafe per-agent, TYPESAFE_API_KEY fallback",
        "llms": ["meta", "deepseek", "openai", "anthropic", "openrouter", "gemini"],
        "apps": ["notion", "supabase", "cloudflare", "minara"]
    }))
}

async fn jev_check(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    let Some(key) = crate::vault::get(&agent, "typesafe") else {
        return Json(json!({"ok": false, "agent": agent, "error": "no jev key"}));
    };
    match crate::jev::check(&key).await {
        Ok(v) => Json(json!({"ok": true, "agent": agent, "jev": v})),
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e.to_string()})),
    }
}

async fn route_decide(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    let task = body.get("task").and_then(|t| t.as_str()).unwrap_or("");
    let Some(key) = crate::vault::get(&agent, "typesafe") else {
        return Json(json!({"ok": false, "agent": agent, "error": "no jev key"}));
    };
    match crate::jev::route(&key, &agent, task).await {
        Ok(v) => Json(json!({"ok": true, "agent": agent, "jev": v})),
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e.to_string()})),
    }
}

const ALL_PROVIDERS: &[&str] = &[
    "typesafe", "meta", "deepseek", "openai", "anthropic", "openrouter", "gemini", "notion",
    "supabase", "cloudflare", "minara",
];

#[derive(Deserialize)]
struct StatusQuery {
    scope: Option<String>,
}

#[derive(Serialize)]
struct ProviderStatus {
    provider: String,
    has_key: bool,
    endpoint: String,
    custom: bool,
}

async fn vault_status(Query(q): Query<StatusQuery>) -> Json<Value> {
    let scope = q.scope.unwrap_or_else(|| "global".into()).to_lowercase();
    let providers: Vec<ProviderStatus> = ALL_PROVIDERS
        .iter()
        .map(|p| ProviderStatus {
            provider: p.to_string(),
            has_key: crate::vault::has(&scope, p),
            endpoint: crate::vault::get_endpoint(&scope, p),
            custom: crate::vault::endpoint_is_custom(&scope, p),
        })
        .collect();
    Json(json!({
        "scope": scope,
        "vault": crate::vault::vault_path().to_string_lossy(),
        "providers": providers,
    }))
}

#[derive(Deserialize)]
struct VaultSet {
    scope: Option<String>,
    provider: String,
    key: Option<String>,
    endpoint: Option<String>,
}

async fn vault_set(Json(body): Json<VaultSet>) -> (StatusCode, Json<Value>) {
    let scope = body.scope.unwrap_or_else(|| "global".into()).to_lowercase();
    let provider = body.provider.to_lowercase();
    if !ALL_PROVIDERS.contains(&provider.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "unknown provider"})),
        );
    }
    if let Some(k) = body.key {
        if k.trim().is_empty() {
            return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "error": "empty key"})));
        }
        if let Err(e) = crate::vault::set(&scope, &provider, k.trim()) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": e.to_string()})),
            );
        }
    }
    if let Some(ep) = body.endpoint {
        if let Err(e) = crate::vault::set_endpoint(&scope, &provider, &ep) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": e.to_string()})),
            );
        }
    }
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "scope": scope,
            "provider": provider,
            "has_key": crate::vault::has(&scope, &provider),
            "endpoint": crate::vault::get_endpoint(&scope, &provider),
            "custom": crate::vault::endpoint_is_custom(&scope, &provider),
        })),
    )
}

#[derive(Deserialize)]
struct NotionSearch {
    query: Option<String>,
}

#[derive(Deserialize)]
struct NotionPage {
    parent_page_id: String,
    title: String,
    markdown: Option<String>,
}

async fn notion_check(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    match crate::notion::check(&agent) {
        Ok(v) => Json(json!({"ok": true, "agent": agent, "me": v})),
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e})),
    }
}

async fn notion_search(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<NotionSearch>,
) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    let q = body.query.unwrap_or_default();
    match crate::notion::search(&agent, &q) {
        Ok(v) => Json(json!({"ok": true, "agent": agent, "results": v})),
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e})),
    }
}

async fn notion_pages(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<NotionPage>,
) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    let md = body.markdown.unwrap_or_default();
    match crate::notion::create_page(&agent, &body.parent_page_id, &body.title, &md) {
        Ok(v) => Json(json!({"ok": true, "agent": agent, "page": v})),
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e})),
    }
}

#[derive(Deserialize)]
struct OpenRouterChat {
    model: String,
    messages: Value,
    max_tokens: Option<u32>,
}

async fn openrouter_check(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    match crate::openrouter::check(&agent) {
        Ok(v) => {
            let n = v.get("data").and_then(|d| d.as_array()).map(|a| a.len()).unwrap_or(0);
            Json(json!({"ok": true, "agent": agent, "models": n}))
        }
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e})),
    }
}

async fn openrouter_chat(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<OpenRouterChat>,
) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    match crate::openrouter::chat(&agent, &body.model, &body.messages, body.max_tokens) {
        Ok(v) => Json(json!({"ok": true, "agent": agent, "completion": v})),
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e})),
    }
}

#[derive(Deserialize)]
struct DeepSeekChat {
    model: String,
    messages: Value,
    max_tokens: Option<u32>,
}

async fn deepseek_check(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    match crate::deepseek::check(&agent) {
        Ok(v) => {
            let n = v.get("data").and_then(|d| d.as_array()).map(|a| a.len()).unwrap_or(0);
            Json(json!({"ok": true, "agent": agent, "models": n}))
        }
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e})),
    }
}

async fn deepseek_chat(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<DeepSeekChat>,
) -> Json<Value> {
    let agent = agent_from(&headers, &st.default_agent);
    match crate::deepseek::chat(&agent, &body.model, &body.messages, body.max_tokens) {
        Ok(v) => Json(json!({"ok": true, "agent": agent, "completion": v})),
        Err(e) => Json(json!({"ok": false, "agent": agent, "error": e})),
    }
}
