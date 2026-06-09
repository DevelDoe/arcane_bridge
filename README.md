# Arcane Bridge

System tray app with an in-process localhost hub (`127.0.0.1:47991`). One executable — no separate hub process.

Monitor is **not** required. Caster and Guilds connect as TCP clients.

## Layout

| Path | Role |
|------|------|
| `backend/` | Tauri tray app + in-process Rust hub |
| `frontend/dist/` | Bridge Console UI (optional window) |
| `hub/` | Legacy Node hub (reference only — not shipped) |

## Dev

```bash
cd arcane_bridge/backend
cargo tauri dev
```

No Node required for dev or release builds.

## Startup order

1. **Arcane Bridge** (this app)
2. Monitor, Caster, Guilds — any order; clients retry until hub is up

## Stale port

```bash
lsof -iTCP:47991 -sTCP:LISTEN
kill $(lsof -ti tcp:47991)
```

On Windows: quit Bridge from the tray, or end any leftover `arcane-bridge-hub.exe` from old installs.

## Release

```bash
./scripts/release.sh patch              # tag + push → CI builds all platforms
./scripts/stage-from-github-release.sh  # after CI: piggyback into other apps
```

See [docs/RELEASE.md](docs/RELEASE.md).
