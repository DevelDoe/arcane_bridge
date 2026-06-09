# Hub — build & deploy

Bundled inside `arcane_bridge`. Dev uses `src/index.js`. Release builds run `npm ci && npm run build:bundle` once before `cargo tauri build`, which verifies the bundle and copies `dist/hub-bundle/` into the Tauri app (`hub/`).

## Dev

Start the tray app (`../backend` → `cargo tauri dev`) or run hub standalone:

```bash
npm install
npm start
```

## Build single file

```bash
npm install
npm run build
# → dist/arcane-bridge.cjs
```

## Build release executable (standalone hub)

```bash
npm install
npm run build:bundle
# → dist/hub-bundle/arcane-bridge-hub (or arcane-bridge-hub.exe on Windows)
```

## Stale port

```bash
kill $(lsof -ti tcp:47991)
```
