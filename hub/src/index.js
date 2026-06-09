#!/usr/bin/env node
import { startBridgeServer } from "./server.js";

function parsePort(raw, fallback) {
    if (raw == null || raw === "") return fallback;
    const n = Number.parseInt(String(raw), 10);
    return Number.isFinite(n) && n > 0 && n <= 65535 ? n : fallback;
}

const host = process.env.ARCANE_BRIDGE_HOST?.trim() || "127.0.0.1";
const port = parsePort(process.env.ARCANE_BRIDGE_PORT, 47991);

const log = (msg) => {
    const ts = new Date().toISOString();
    console.log(`${ts} ${msg}`);
};

log("[arcane-bridge] starting central node…");

try {
    await startBridgeServer({ host, port, log });
} catch (err) {
    console.error("[arcane-bridge] failed to start:", err?.message || err);
    process.exit(1);
}
