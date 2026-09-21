//! Porter native settings UI (GPUI).
//! Tabs: General / Connectors / Agents. Talks to the porter daemon on
//! 127.0.0.1:8819 (blocking localhost calls; no secrets are ever displayed).

use gpui::*;
use std::collections::HashMap;

const GROUPS: &[(&str, &[&str])] = &[
    ("Jev", &["typesafe"]),
    ("LLMs", &["meta", "deepseek", "openai", "anthropic", "openrouter", "gemini"]),
    ("Apps", &["notion", "supabase", "cloudflare", "minara"]),
];
const AGENTS: &[&str] = &["codex", "hermes", "omp"];
const SCOPES: &[&str] = &["global", "codex", "hermes", "omp"];

fn api_base() -> String {
    std::env::var("PORTER_BASE").unwrap_or_else(|_| "http://127.0.0.1:8819".into())
}

#[derive(Clone, Debug, Default)]
struct ProviderState {
    provider: String,
    has_key: bool,
    endpoint: String,
    custom: bool,
}

fn fetch_health() -> bool {
    reqwest::blocking::get(format!("{}/health", api_base()))
        .ok()
        .and_then(|r| r.json::<serde_json::Value>().ok())
        .and_then(|v| v.get("ok").and_then(|o| o.as_bool()))
        .unwrap_or(false)
}

fn fetch_status(scope: &str) -> Result<(Vec<ProviderState>, String), String> {
    let v: serde_json::Value = reqwest::blocking::get(format!(
        "{}/v1/vault/status?scope={scope}",
        api_base()
    ))
    .map_err(|e| e.to_string())?
    .json()
    .map_err(|e| e.to_string())?;
    let providers = v
        .get("providers")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|p| ProviderState {
            provider: p.get("provider").and_then(|s| s.as_str()).unwrap_or("").into(),
            has_key: p.get("has_key").and_then(|b| b.as_bool()).unwrap_or(false),
            endpoint: p.get("endpoint").and_then(|s| s.as_str()).unwrap_or("").into(),
            custom: p.get("custom").and_then(|b| b.as_bool()).unwrap_or(false),
        })
        .collect();
    let vault = v.get("vault").and_then(|s| s.as_str()).unwrap_or("").into();
    Ok((providers, vault))
}

fn post_set(scope: &str, provider: &str, key: Option<&str>, endpoint: Option<&str>) -> Result<String, String> {
    let mut body = serde_json::json!({"scope": scope, "provider": provider});
    if let Some(k) = key {
        body["key"] = serde_json::Value::String(k.into());
    }
    if let Some(e) = endpoint {
        body["endpoint"] = serde_json::Value::String(e.into());
    }
    let v: serde_json::Value = reqwest::blocking::Client::new()
        .post(format!("{}/v1/vault/set", api_base()))
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    if v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
        Ok("saved".into())
    } else {
        Err(v.get("error").and_then(|s| s.as_str()).unwrap_or("save failed").into())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Connectors,
    Agents,
}

struct PorterUi {
    focus: FocusHandle,
    tab: Tab,
    scope: String,
    daemon_ok: bool,
    providers: Vec<ProviderState>,
    vault_path: String,
    agents_jev: Vec<(String, bool)>,
    fields: HashMap<String, String>,
    carets: HashMap<String, usize>,
    focused: Option<String>,
    toast: Option<String>,
}

impl PorterUi {
    fn new(cx: &mut Context<Self>) -> Self {
        let daemon_ok = fetch_health();
        let (providers, vault_path) = fetch_status("global").unwrap_or_default();
        let mut this = Self {
            focus: cx.focus_handle(),
            tab: Tab::Connectors,
            scope: "global".into(),
            daemon_ok,
            providers,
            vault_path,
            agents_jev: Vec::new(),
            fields: HashMap::new(),
            carets: HashMap::new(),
            focused: None,
            toast: if daemon_ok { None } else { Some("daemon not reachable on 127.0.0.1:8819".into()) },
        };
        this.reload(cx);
        this
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.daemon_ok = fetch_health();
        match fetch_status(&self.scope) {
            Ok((providers, vault)) => {
                self.providers = providers;
                self.vault_path = vault;
            }
            Err(e) => self.toast = Some(format!("status failed: {e}")),
        }
        let mut agents = Vec::new();
        for a in AGENTS {
            let has = fetch_status(a)
                .map(|(ps, _)| ps.into_iter().any(|p| p.provider == "typesafe" && p.has_key))
                .unwrap_or(false);
            agents.push((a.to_string(), has));
        }
        self.agents_jev = agents;
        cx.notify();
    }

    fn save_key(&mut self, provider: &str, cx: &mut Context<Self>) {
        let id = format!("key:{provider}");
        let value = self.fields.get(&id).cloned().unwrap_or_default();
        if value.trim().is_empty() {
            self.toast = Some("paste a key first".into());
            cx.notify();
            return;
        }
        match post_set(&self.scope, provider, Some(value.trim()), None) {
            Ok(_) => {
                self.toast = Some(format!("{provider} key saved"));
                self.fields.insert(id, String::new());
            }
            Err(e) => self.toast = Some(format!("save failed: {e}")),
        }
        self.reload(cx);
    }

    fn save_endpoint(&mut self, provider: &str, cx: &mut Context<Self>) {
        let id = format!("ep:{provider}");
        let value = self.fields.get(&id).cloned().unwrap_or_else(|| {
            self.providers.iter().find(|p| p.provider == provider).map(|p| p.endpoint.clone()).unwrap_or_default()
        });
        match post_set(&self.scope, provider, None, Some(&value)) {
            Ok(_) => self.toast = Some(format!("{provider} endpoint saved")),
            Err(e) => self.toast = Some(format!("save failed: {e}")),
        }
        self.reload(cx);
    }

    fn reset_endpoint(&mut self, provider: &str, cx: &mut Context<Self>) {
        match post_set(&self.scope, provider, None, Some("")) {
            Ok(_) => self.toast = Some(format!("{provider} endpoint reset")),
            Err(e) => self.toast = Some(format!("reset failed: {e}")),
        }
        self.reload(cx);
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(field) = self.focused.clone() else { return };
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;
        if key == "escape" {
            self.focused = None;
            cx.notify();
            return;
        }
        if (mods.control || mods.platform) && key == "v" {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                if let Ok(text) = cb.get_text() {
                    self.insert_text(&field, &text);
                    cx.notify();
                }
            }
            return;
        }
        if mods.control || mods.platform || mods.alt {
            return;
        }
        match key {
            "backspace" => {
                let caret = self.carets.get(&field).copied().unwrap_or(0);
                let text = self.fields.entry(field.clone()).or_default();
                let mut chars: Vec<char> = text.chars().collect();
                if caret > 0 && caret <= chars.len() {
                    chars.remove(caret - 1);
                    *text = chars.into_iter().collect();
                    self.carets.insert(field, caret - 1);
                }
                cx.notify();
            }
            "delete" => {
                let caret = self.carets.get(&field).copied().unwrap_or(0);
                let text = self.fields.entry(field.clone()).or_default();
                let mut chars: Vec<char> = text.chars().collect();
                if caret < chars.len() {
                    chars.remove(caret);
                    *text = chars.into_iter().collect();
                }
                cx.notify();
            }
            "left" => {
                let caret = self.carets.get(&field).copied().unwrap_or(0);
                self.carets.insert(field, caret.saturating_sub(1));
                cx.notify();
            }
            "right" => {
                let caret = self.carets.get(&field).copied().unwrap_or(0);
                let len = self.fields.get(&field).map(|s| s.chars().count()).unwrap_or(0);
                self.carets.insert(field, (caret + 1).min(len));
                cx.notify();
            }
            "enter" => {
                if let Some((kind, provider)) = field.split_once(':') {
                    let provider = provider.to_string();
                    match kind {
                        "key" => self.save_key(&provider, cx),
                        "ep" => self.save_endpoint(&provider, cx),
                        _ => {}
                    }
                }
            }
            _ => {
                let typed = event.keystroke.key_char.clone().or_else(|| {
                    if key.chars().count() == 1 { Some(key.to_string()) } else { None }
                });
                if let Some(t) = typed {
                    self.insert_text(&field, &t);
                    cx.notify();
                }
            }
        }
    }

    fn insert_text(&mut self, field: &str, text: &str) {
        let caret = self.carets.get(field).copied().unwrap_or(usize::MAX);
        let entry = self.fields.entry(field.to_string()).or_default();
        let mut chars: Vec<char> = entry.chars().collect();
        let caret = caret.min(chars.len());
        let insert: Vec<char> = text.chars().collect();
        let n = insert.len();
        chars.splice(caret..caret, insert);
        *entry = chars.into_iter().collect();
        self.carets.insert(field.to_string(), caret + n);
    }

    fn field_text(&self, id: &str, fallback: &str) -> String {
        if self.focused.as_deref() == Some(id) {
            let text = self.fields.get(id).cloned().unwrap_or_default();
            let caret = self.carets.get(id).copied().unwrap_or(text.chars().count());
            let mut chars: Vec<char> = text.chars().collect();
            chars.insert(caret.min(chars.len()), '|');
            chars.into_iter().collect()
        } else {
            self.fields.get(id).cloned().unwrap_or_else(|| fallback.to_string())
        }
    }
}

fn dark() -> Rgba { rgb(0x1e1e22) }
fn panel() -> Rgba { rgb(0x26262c) }
fn line() -> Rgba { rgb(0x3a3a42) }
fn ink() -> Rgba { rgb(0xe8e8ea) }
fn dim() -> Rgba { rgb(0x9a9aa2) }
fn accent() -> Rgba { rgb(0x7aa2f7) }
fn good() -> Rgba { rgb(0x73d08c) }
fn bad() -> Rgba { rgb(0xe07070) }

impl Render for PorterUi {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.tab;
        div()
            .id("porter-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(dark())
            .text_color(ink())
            .track_focus(&self.focus)
            .on_key_down(window.listener_for(&cx.entity(), |this, event, _window, cx| {
                this.on_key(event, cx);
            }))
            .child(
                div()
                    .id("header")
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(line())
                    .child(div().text_size(rems(1.1)).child("Porter"))
                    .child(
                        div()
                            .text_size(rems(0.75))
                            .text_color(if self.daemon_ok { good() } else { bad() })
                            .child(if self.daemon_ok { "● daemon" } else { "● daemon down" }),
                    )
                    .child(tab_btn("tab-general", "General", tab == Tab::General, window, cx, Tab::General))
                    .child(tab_btn("tab-connectors", "Connectors", tab == Tab::Connectors, window, cx, Tab::Connectors))
                    .child(tab_btn("tab-agents", "Agents", tab == Tab::Agents, window, cx, Tab::Agents)),
            )
            .child(match tab {
                Tab::General => self.render_general(window, cx),
                Tab::Connectors => self.render_connectors(window, cx),
                Tab::Agents => self.render_agents(window, cx),
            })
            .children(self.toast.clone().map(|message| {
                div()
                    .id("toast")
                    .m_4()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(panel())
                    .border_1()
                    .border_color(line())
                    .text_size(rems(0.8))
                    .child(message)
            }))
    }
}

fn tab_btn(
    id: &'static str,
    label: &'static str,
    active: bool,
    window: &mut Window,
    cx: &mut Context<PorterUi>,
    tab: Tab,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .bg(if active { accent() } else { panel() })
        .text_color(if active { rgb(0x101014) } else { ink() })
        .on_click(window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
            this.tab = tab;
            this.reload(cx);
        }))
        .child(label)
}

fn action_btn(
    id: String,
    label: String,
    window: &mut Window,
    cx: &mut Context<PorterUi>,
    run: impl Fn(&mut PorterUi, &mut Context<PorterUi>) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .px_2()
        .py_1()
        .rounded_md()
        .bg(panel())
        .border_1()
        .border_color(line())
        .text_size(rems(0.75))
        .on_click(window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
            run(this, cx);
        }))
        .child(label)
}

impl PorterUi {
    fn render_general(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let vault = self.vault_path.clone();
        div()
            .id("tab-general")
            .flex_1()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().child("Local API holder + Jev-gated router."))
            .child(div().text_size(rems(0.8)).text_color(dim()).child(format!("Vault file: {vault} (0600)")))
            .child(div().text_size(rems(0.8)).text_color(dim()).child("Port 8819 · systemd user unit porter.service"))
            .child(div().text_size(rems(0.8)).text_color(dim()).child("Keys are write-only here: presence dots only, values never shown."))
            .child(action_btn("refresh".into(), "Refresh".into(), window, cx, |this, cx| this.reload(cx)))
    }

    fn render_connectors(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let scope = self.scope.clone();
        let mut body = div()
            .id("tab-connectors")
            .flex_1()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_size(rems(0.8)).text_color(dim()).child("Scope:"))
                    .children(SCOPES.iter().map(|s| {
                        let s = s.to_string();
                        let active = s == scope;
                        let next = s.clone();
                        div()
                            .id(SharedString::from(format!("scope-{s}")))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(if active { accent() } else { panel() })
                            .text_color(if active { rgb(0x101014) } else { ink() })
                            .text_size(rems(0.8))
                            .on_click(window.listener_for(&cx.entity(), move |this, _event, _window, cx| {
                                this.scope = next.clone();
                                this.focused = None;
                                this.reload(cx);
                            }))
                            .child(s)
                    }))
                    .child(div().text_size(rems(0.75)).text_color(dim()).child("per-agent overrides global · Jev falls back to TYPESAFE_API_KEY")),
            );
        for (group, items) in GROUPS {
            body = body.child(
                div().text_size(rems(0.8)).text_color(dim()).child(group.to_string()),
            );
            for provider in *items {
                let p = provider.to_string();
                let st = self.providers.iter().find(|x| x.provider == p).cloned().unwrap_or_default();
                let key_id = format!("key:{p}");
                let ep_id = format!("ep:{p}");
                let key_shown = if self.focused.as_deref() == Some(&key_id) {
                    self.field_text(&key_id, "")
                } else {
                    let v = self.fields.get(&key_id).cloned().unwrap_or_default();
                    if v.is_empty() {
                        String::new()
                    } else {
                        "•".repeat(v.chars().count())
                    }
                };
                let ep_shown = self.fields.get(&ep_id).cloned().unwrap_or_else(|| {
                    if self.focused.as_deref() == Some(&ep_id) {
                        self.field_text(&ep_id, "")
                    } else {
                        st.endpoint.clone()
                    }
                });
                let ep_shown = if self.focused.as_deref() == Some(&ep_id) {
                    self.field_text(&ep_id, "")
                } else {
                    ep_shown
                };
                let dot = if st.has_key { "●" } else { "○" };
                let dot_color = if st.has_key { good() } else { bad() };
                let key_ph = if st.has_key { "•••••• (saved)" } else { "paste key, Enter saves" };
                let p_save = p.clone();
                let p_save_ep = p.clone();
                let p_reset = p.clone();
                let key_id_c = key_id.clone();
                let ep_id_c = ep_id.clone();
                let custom = st.custom;
                body = body.child(
                    div()
                        .id(SharedString::from(format!("row-{p}")))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(panel())
                        .child(div().text_color(dot_color).child(dot.to_string()))
                        .child(div().w(px(90.0)).text_size(rems(0.85)).child(p.clone()))
                        .child(
                            div()
                                .id(SharedString::from(format!("field-{key_id}")))
                                .flex_1()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(dark())
                                .border_1()
                                .border_color(if self.focused.as_deref() == Some(&key_id) { accent() } else { line() })
                                .text_size(rems(0.8))
                                .text_color(if key_shown.is_empty() { dim() } else { ink() })
                                .on_click(window.listener_for(&cx.entity(), move |this, _event, window, cx| {
                                    this.focused = Some(key_id_c.clone());
                                    window.focus(&this.focus);
                                    cx.notify();
                                }))
                                .child(if key_shown.is_empty() { key_ph.to_string() } else { key_shown }),
                        )
                        .child(action_btn(format!("savekey-{p}"), "Save key".into(), window, cx, move |this, cx| {
                            this.save_key(&p_save, cx);
                        }))
                        .child(
                            div()
                                .id(SharedString::from(format!("field-{ep_id}")))
                                .flex_1()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(dark())
                                .border_1()
                                .border_color(if self.focused.as_deref() == Some(&ep_id) { accent() } else { line() })
                                .text_size(rems(0.75))
                                .on_click(window.listener_for(&cx.entity(), move |this, _event, window, cx| {
                                    this.focused = Some(ep_id_c.clone());
                                    window.focus(&this.focus);
                                    cx.notify();
                                }))
                                .child(if ep_shown.is_empty() { "(default)".to_string() } else { ep_shown }),
                        )
                        .child(action_btn(format!("saveep-{p}"), "Save URL".into(), window, cx, move |this, cx| {
                            this.save_endpoint(&p_save_ep, cx);
                        }))
                        .children(custom.then(|| {
                            action_btn(format!("resetep-{p}"), "Reset".into(), window, cx, move |this, cx| {
                                this.reset_endpoint(&p_reset, cx);
                            })
                        })),
                );
            }
        }
        body
    }

    fn render_agents(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut body = div()
            .id("tab-agents")
            .flex_1()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_size(rems(0.8)).text_color(dim()).child("Each agent uses its own Jev key, else the shared global one."));
        for (agent, has) in self.agents_jev.clone() {
            body = body.child(
                div()
                    .id(SharedString::from(format!("agent-{agent}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(panel())
                    .child(div().text_color(if has { good() } else { bad() }).child(if has { "●" } else { "○" }))
                    .child(div().w(px(90.0)).child(agent))
                    .child(div().text_size(rems(0.8)).text_color(dim()).child(if has { "jev key set" } else { "missing" })),
            );
        }
        body = body.child(action_btn("agents-refresh".into(), "Refresh".into(), window, cx, |this, cx| this.reload(cx)));
        body
    }
}

fn main() {
    if std::env::args().any(|a| a == "--smoke") {
        let ok = fetch_health();
        let (providers, vault) = fetch_status("global").unwrap_or_default();
        println!("PORTER_UI_SMOKE daemon_ok={ok} providers={} vault={vault}", providers.len());
        for p in &providers {
            println!("  {} has_key={} custom={}", p.provider, p.has_key, p.custom);
        }
        println!("PORTER_UI_SMOKE_OK");
        return;
    }
    Application::new().run(move |app: &mut App| {
        app.activate(true);
        let opts = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some("Porter".into()), ..Default::default() }),
            app_id: Some("porter-ui".to_string()),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1060.0), px(720.0)),
                app,
            ))),
            ..Default::default()
        };
        app.open_window(opts, |window, app| {
            let entity = app.new(|cx| PorterUi::new(cx));
            window.focus(&entity.read(app).focus);
            entity
        })
        .expect("open Porter window");
    });
}
