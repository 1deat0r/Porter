# Porter
Local API holder + Jev-gated router for AI agents, with a native settings UI.

- Agents (`codex`, `hermes`, `omp`) call `127.0.0.1:8819`; porter injects real keys. Raw keys never leave the vault (`~/.config/porter/vault.json`, `0600`).
- Jev (`jev-latest`) makes fast typed routing calls: which LLM, does this need human confirm.
- `porter/` — Rust + axum daemon and CLI. Providers: typesafe/jev, notion, openrouter, deepseek; slots for meta, openai, anthropic, gemini, supabase, cloudflare, minara.
- `porter-ui/` — native GPUI settings window (General / Connectors / Agents).

```bash
cargo run -p porter -- key has codex typesafe
VAULT_VALUE=... cargo run -p porter -- key set codex typesafe
cargo run -p porter -- serve --port 8819
cargo run -p porter-ui -- --smoke
```

See `porter/AGENTS.md` and `porter-ui/AGENTS.md` for agent guides.
