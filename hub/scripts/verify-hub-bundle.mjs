#!/usr/bin/env node
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const bundleDir = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "dist", "hub-bundle");
const names =
    process.platform === "win32"
        ? ["arcane-bridge-hub.exe"]
        : ["arcane-bridge-hub"];

for (const name of names) {
    const file = path.join(bundleDir, name);
    if (fs.existsSync(file)) {
        process.exit(0);
    }
}

console.error(`[hub-bundle] missing hub executable in ${bundleDir} — run: npm ci && npm run build:bundle`);
process.exit(1);
