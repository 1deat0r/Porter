# porter-ui guide
- gpui 0.2.2 Entity API (mirrors BB-GPUI patterns). Single file `src/main.rs`.
- No built-in text input in gpui 0.2: focused-field map + root `on_key_down` + caret; paste via `arboard` (gpui ClipboardItem has no text getter here).
- HTTP is blocking reqwest to localhost (fast); `--smoke` verifies without a window.
- Checks: `cargo run -- --smoke`, window launch on `:0`.
