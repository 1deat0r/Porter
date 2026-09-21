# porter (Rust, Ubuntu LTS)
Local API holder + Jev-gated router. Agents (codex/hermes/omp) call `127.0.0.1:8819`, vault injects real keys. Raw keys never leave the vault.

- Jev keys per-agent (`codex/typesafe`, `hermes/typesafe`, `omp/typesafe`), fallback `TYPESAFE_API_KEY`.
- LLMs: `meta` (muse-spark default), `deepseek`, `openai`, `anthropic`, `openrouter`, `gemini`. Apps: `notion`, `supabase`, `cloudflare`, `minara`.
- Jev decides routing: one `POST https://api.typesafe.ai/v1/systemone` with `model jev-latest`, `Choice route_llm` + `Noul needs_escalation`.

```bash
cargo run -- key has codex typesafe
VAULT_VALUE=... cargo run -- key set codex typesafe
cargo run -- jev check --agent codex
cargo run -- route --agent codex --task "summarize inbox"
cargo run -- serve --port 8819
```
Store: `~/.config/porter/vault.json` (`0600`). Env fallback for dev only.

## Settings UI (Connectors tab)
- `http://127.0.0.1:8819/settings` — General | Connectors | Agents.
- Connectors scope: `global` shared or per-agent (`codex`/`hermes`/`omp` override global; Jev falls back to `TYPESAFE_API_KEY`).
- Paste API keys (never echoed back, presence dot only) and endpoints (custom or Reset to default) per connector.
- GNOME launcher: Agent Vault Settings (opens the page).
