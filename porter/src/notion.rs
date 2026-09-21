//! Notion provider: PAT auth (Bearer + Notion-Version), search, page create.
//! The API takes blocks, not markdown — porter converts a small markdown
//! subset (headings, bullets, numbered, quotes, code fences, paragraphs).

use serde_json::{json, Value};

pub const VERSION: &str = "2022-06-28";
const DEFAULT_BASE: &str = "https://api.notion.com/v1";
const MAX_CHILDREN: usize = 100;

fn base_for(agent: &str) -> String {
    let ep = crate::vault::get_endpoint(agent, "notion");
    let ep = ep.trim().trim_end_matches('/').to_string();
    if ep.is_empty() {
        DEFAULT_BASE.into()
    } else {
        ep
    }
}

fn call(method: &str, path: &str, agent: &str, body: Option<Value>) -> Result<Value, String> {
    let key = crate::vault::get(agent, "notion").ok_or_else(|| "no notion key".to_string())?;
    let url = format!("{}{}", base_for(agent), path);
    let c = reqwest::blocking::Client::new();
    let mut r = match method {
        "GET" => c.get(&url),
        _ => c.post(&url),
    };
    r = r.bearer_auth(&key).header("Notion-Version", VERSION);
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

fn map_error(status: u16, v: &Value) -> String {
    let code = v.get("code").and_then(|c| c.as_str()).unwrap_or("");
    let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
    match code {
        "unauthorized" => "unauthorized: bad or expired token — repaste the PAT".into(),
        "restricted_resource" => "restricted_resource: token lacks a capability — enable it on the integration".into(),
        "object_not_found" => "object_not_found: bad parent id, or page not shared with the integration".into(),
        "rate_limited" => "rate_limited: back off and retry".into(),
        _ => format!("notion {status} {code}: {msg}"),
    }
}

/// Key test: GET /v1/users/me.
pub fn check(agent: &str) -> Result<Value, String> {
    call("GET", "/users/me", agent, None)
}

/// Title/content search, 10 hits.
pub fn search(agent: &str, query: &str) -> Result<Value, String> {
    call("POST", "/search", agent, Some(json!({"query": query, "page_size": 10})))
}

/// Create a page under a parent page id, body converted from markdown.
pub fn create_page(agent: &str, parent: &str, title: &str, markdown: &str) -> Result<Value, String> {
    call(
        "POST",
        "/pages",
        agent,
        Some(json!({
            "parent": {"page_id": parent},
            "properties": {"title": [{"text": {"content": title}}]},
            "children": md_to_blocks(markdown),
        })),
    )
}

fn rich(text: &str) -> Value {
    json!([{"text": {"content": text}}])
}

fn block_for_line(t: &str) -> Value {
    if let Some(h) = t.strip_prefix("### ") {
        return json!({"type": "heading_3", "heading_3": {"rich_text": rich(h)}});
    }
    if let Some(h) = t.strip_prefix("## ") {
        return json!({"type": "heading_2", "heading_2": {"rich_text": rich(h)}});
    }
    if let Some(h) = t.strip_prefix("# ") {
        return json!({"type": "heading_1", "heading_1": {"rich_text": rich(h)}});
    }
    if let Some(q) = t.strip_prefix("> ") {
        return json!({"type": "quote", "quote": {"rich_text": rich(q)}});
    }
    if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
        return json!({"type": "bulleted_list_item", "bulleted_list_item": {"rich_text": rich(b)}});
    }
    if let Some((n, rest)) = t.split_once(". ") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            return json!({"type": "numbered_list_item", "numbered_list_item": {"rich_text": rich(rest)}});
        }
    }
    json!({"type": "paragraph", "paragraph": {"rich_text": rich(t)}})
}

pub fn md_to_blocks(md: &str) -> Vec<Value> {
    let mut blocks = Vec::new();
    let mut fence: Option<Vec<String>> = None;
    for line in md.lines() {
        if let Some(f) = fence.as_mut() {
            if line.trim_start().starts_with("```") {
                blocks.push(json!({"type": "code", "code": {"rich_text": rich(&f.join("\n")), "language": "plain text"}}));
                fence = None;
            } else {
                f.push(line.to_string());
            }
            continue;
        }
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("```") {
            fence = Some(Vec::new());
            continue;
        }
        blocks.push(block_for_line(t));
        if blocks.len() >= MAX_CHILDREN {
            break;
        }
    }
    if let Some(f) = fence {
        blocks.push(json!({"type": "code", "code": {"rich_text": rich(&f.join("\n")), "language": "plain text"}}));
    }
    blocks.truncate(MAX_CHILDREN);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_subset() {
        let b = md_to_blocks("# A\n## B\n- x\n1. y\n> q\n```\ncode\n```\nplain");
        let types: Vec<&str> = b.iter().map(|v| v["type"].as_str().unwrap()).collect();
        assert_eq!(
            types,
            vec!["heading_1", "heading_2", "bulleted_list_item", "numbered_list_item", "quote", "code", "paragraph"]
        );
    }

    #[test]
    fn unclosed_fence_becomes_code() {
        let b = md_to_blocks("```\nloose");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0]["type"], "code");
    }

    #[test]
    fn caps_at_100() {
        let md = (0..150).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        assert_eq!(md_to_blocks(&md).len(), MAX_CHILDREN);
    }
}
