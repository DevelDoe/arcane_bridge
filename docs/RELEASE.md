# Arcane Bridge — releases

## 1. Release

```bash
git pull
cd arcane_bridge
./scripts/release.sh          # patch (default)
./scripts/release.sh minor
./scripts/release.sh major
```

Commits everything, bumps version, tags, pushes. CI builds Win/Mac/Linux.

## 2. Stage (CI green)

```bash
./scripts/stage-from-github-release.sh
```

Installers → Monitor / Caster / Guilds `resources/bridge/`.

## 3. Ship apps

Commit `resources/bridge/` in each app. Build on your hosts.
