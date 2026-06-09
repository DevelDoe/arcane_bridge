# Arcane Bridge

Menu bar app + localhost hub (`127.0.0.1:47991`). One install — tray starts the hub and shows connected apps.

Monitor is **not** required. Caster and Guilds connect as clients.

## Layout

| Path | Role |
|------|------|
| `backend/` | Tauri tray app (Rust) |
| `hub/` | Node TCP hub (spawned by tray) |
| `frontend/dist/` | Minimal shell (tray-only) |

## Dev

```bash
cd arcane_bridge/backend
cargo tauri dev
```

Dev requires **Node 18+** on `PATH`. Production builds bundle `hub/arcane-bridge.mjs` inside the app (still needs Node on `PATH`).

Hub only (debug):

```bash
cd arcane_bridge/hub
npm install
npm start
```

## Startup order

1. **Arcane Bridge** (this app)
2. Monitor, Caster, Guilds — any order after hub is up

## Stale port

```bash
lsof -iTCP:47991 -sTCP:LISTEN
kill $(lsof -ti tcp:47991)
```

## Build hub bundle

```bash
cd hub && npm install && npm run build
# → hub/dist/arcane-bridge.mjs
```

See [hub/BUILD.md](hub/BUILD.md).

## Release

```bash
./scripts/release.sh patch              # tag + push → CI builds all platforms
./scripts/stage-from-github-release.sh  # after CI: piggyback into other apps
```

See [docs/RELEASE.md](docs/RELEASE.md).
