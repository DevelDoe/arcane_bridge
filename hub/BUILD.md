# Hub (Node) — legacy reference

Production Bridge runs the TCP hub **in-process** inside `../backend/` (Rust). This Node package is kept for protocol reference and local debugging only.

## Normal dev

```bash
cd ../backend && cargo tauri dev
```

## Debug Node hub (optional)

```bash
npm install
npm start
```

## Build standalone bundle (not shipped in releases)

```bash
npm run build        # dist/arcane-bridge.cjs
npm run build:bundle # dist/hub-bundle/ — legacy, not used by Tauri builds
```

## Stale port

```bash
kill $(lsof -ti tcp:47991)
```
