# Porter guide
- Crates: `porter/` (daemon+CLI, binary `porter`), `porter-ui/` (GPUI window, binary `porter-ui`). No workspace; build each crate in its dir.
- Never log, print, or commit secret values. `key list` and `/v1/providers` expose names only. Secrets live in `~/.config/porter/vault.json`, never in this repo.
- Daemon checks: `cargo test`, `key has <agent> typesafe`, live `jev check`. UI checks: `--smoke`, window launch on `:0`.
- Ubuntu service: `porter.service` (systemd user unit) runs the installed `porter` binary on port 8819.
