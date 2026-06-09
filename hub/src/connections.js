/** Track Arcane app roles on TCP sockets for admin / tray UI. */

/**
 * @param {string | null | undefined} id
 * @returns {"monitor" | "caster" | "guilds" | "bridge_app" | "unknown"}
 */
export function inferRoleFromClientId(id) {
    const s = String(id ?? "").trim().toLowerCase();
    if (!s) return "unknown";
    if (s.includes("monitor")) return "monitor";
    if (s.includes("caster")) return "caster";
    if (s.includes("guilds")) return "guilds";
    if (s.includes("bridge") || s.includes("admin")) return "bridge_app";
    return "unknown";
}

/**
 * @param {import("net").Socket} socket
 * @param {"monitor" | "caster" | "guilds" | "bridge_app" | "unknown"} role
 * @param {string} [clientId]
 */
export function tagSocket(socket, role, clientId) {
    if (!socket) return;
    const prev = socket._bridgeMeta || {};
    socket._bridgeMeta = {
        role: role || prev.role || "unknown",
        clientId: clientId != null && String(clientId).trim() !== "" ? String(clientId).trim() : prev.clientId || null,
        connectedAt: prev.connectedAt || Date.now(),
    };
}

/**
 * @param {import("net").Socket | null | undefined} socket
 */
export function socketClientEntry(socket) {
    if (!socket || socket.destroyed) return null;
    const m = socket._bridgeMeta || {};
    return {
        role: m.role || "unknown",
        id: m.clientId || "client",
        connectedAt: m.connectedAt || null,
    };
}

/**
 * @param {{
 *   getMonitorPublisher: () => import("net").Socket | null,
 *   watchlistSubscribers: Set<import("net").Socket>,
 * }} ctx
 * @param {string} host
 * @param {number} port
 */
export function connectionsSnapshot(ctx, host, port) {
    /** @type {Map<import("net").Socket, { role: string, id: string, connectedAt: number | null }>} */
    const seen = new Map();

    const pub = ctx.getMonitorPublisher();
    const pubEntry = socketClientEntry(pub);
    if (pub && pubEntry) {
        seen.set(pub, {
            role: "monitor",
            id: pubEntry.id || "monitor-publisher",
            connectedAt: pubEntry.connectedAt,
        });
    }

    for (const s of ctx.watchlistSubscribers) {
        if (!s || s.destroyed || seen.has(s)) continue;
        const entry = socketClientEntry(s);
        if (!entry) continue;
        seen.set(s, {
            role: entry.role,
            id: entry.id,
            connectedAt: entry.connectedAt,
        });
    }

    const clients = [...seen.values()];
    const apps = {
        monitor: clients.some((c) => c.role === "monitor"),
        caster: clients.some((c) => c.role === "caster"),
        guilds: clients.some((c) => c.role === "guilds"),
    };

    return {
        listening: true,
        host,
        port,
        apps,
        clients,
    };
}
