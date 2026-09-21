# porter-ui (GPUI, native)
Native settings window for the porter daemon: General / Connectors / Agents tabs.

```bash
cargo run -- --smoke        # daemon check, no window
cargo run                   # open window (X11/XWayland on this box)
cargo install --path .
```

- Scope picker: `global` shared, per-agent override; Jev falls back to `TYPESAFE_API_KEY`.
- Fields: click to focus, type, `Ctrl+V` paste, `Enter` saves, `Esc` blurs. Keys masked after save; values never displayed.
- Backend: blocking localhost calls to `PORTER_BASE` (default `http://127.0.0.1:8819`).
