# porter guide
- Rust + axum on `127.0.0.1:8819`. `src/vault.rs` = storage, `src/jev.rs` = TypeSafe call, `src/proxy.rs` = routes.
- Never log/print secret values. `key list` and `/v1/providers` expose names only.
- Jev body: `{model:"jev-latest", state, questions:{id:{type,instructions,criteria}}}`. Choice criteria = option map (<=255), Noul optional true/false.
- Add provider: extend names in `proxy.rs`; lookup auto-works via `vault::get`.
- Checks: `cargo run -- key has <agent> typesafe`, `cargo run -- jev check --agent codex`.
- UI: `src/ui.rs` (tabs), `GET /settings`, `GET /v1/vault/status?scope=`, `POST /v1/vault/set {scope,provider,key?,endpoint?}`. Empty endpoint clears custom. Unknown provider / empty key rejected.
- Notion: `src/notion.rs` (PAT Bearer + `Notion-Version: 2022-06-28`, base from vault endpoint or default). Routes: `POST /v1/notion/check|search|pages`. API takes blocks — `md_to_blocks` converts headings/bullets/numbered/quotes/code/paragraphs, 100 cap. Tests: `cargo test notion`.
- OpenRouter: `src/openrouter.rs` (Bearer, base from vault or default). Routes: `POST /v1/openrouter/check` (model count) + `/v1/openrouter/chat {model,messages,max_tokens?}`. Tests: `cargo test openrouter`.
- DeepSeek: `src/deepseek.rs` (direct API, Bearer). Routes: `POST /v1/deepseek/check` + `/v1/deepseek/chat {model,messages,max_tokens?}`. Tests: `cargo test deepseek`.
