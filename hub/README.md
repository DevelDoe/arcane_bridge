# Hub (Node) — legacy reference

The production hub is **in-process Rust** inside `../backend/`. This Node package is kept for protocol reference and local debugging only — it is **not** bundled in releases.

```bash
npm install
npm start   # only if you need to compare against the old Node hub
```

TCP/JSONL on `127.0.0.1:47991`. Use the Arcane Bridge tray app for normal development:

```bash
cd ../backend && cargo tauri dev
```
