# Hub — build & deploy

Bundled inside `arcane_bridge`. Dev uses `src/index.js`. Release builds copy `dist/arcane-bridge.mjs` into the Tauri app (`resources/hub/arcane-bridge.mjs`) via `beforeBuildCommand` + `bundle.resources`.

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
# → dist/arcane-bridge.mjs
```

## Stale port

```bash
kill $(lsof -ti tcp:47991)
```
