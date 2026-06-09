import net from "net";
import { connectionsSnapshot } from "./connections.js";
import { handleMessage, onMonitorPublisherDisconnect } from "./protocol.js";
import {
    casterAccountPayload,
    casterJournalPayload,
    mergeWatchlistPublish,
    vaultPayload,
    watchlistPayload,
} from "./state.js";

function safeParseLine(line) {
    const trimmed = String(line || "").trim();
    if (!trimmed) return null;
    try {
        return JSON.parse(trimmed);
    } catch {
        return null;
    }
}

function writeJsonLine(socket, obj) {
    if (!socket || socket.destroyed || !socket.writable) return;
    socket.write(`${JSON.stringify(obj)}\n`);
}

const FORWARD_TO_MONITOR = new Set([
    "watchlist.add",
    "watchlist.remove",
    "activeHero.toggle",
    "activeHero.setEnabled",
]);

/** Inbound from Monitor publisher socket (after monitor.register). */
const MONITOR_PUBLISH_TYPES = new Set([
    "monitor.register",
    "session.publish",
    "watchlist.publish",
    "vault.publish",
]);

/**
 * @param {{ host: string, port: number, log: (msg: string) => void }} opts
 */
export function startBridgeServer(opts) {
    const { host, port, log } = opts;
    const watchlistSubscribers = new Set();
    const vaultSubscribers = new Set();
    const casterAccountSubscribers = new Set();
    const casterJournalSubscribers = new Set();
    const casterTicketSubscribers = new Set();
    const adminSubscribers = new Set();
    /** @type {import("net").Socket | null} */
    let monitorPublisher = null;
    /** @type {Map<string, import("net").Socket>} */
    const pendingByRequestId = new Map();
    let monitorBuf = "";

    const broadcastWatchlist = () => {
        if (watchlistSubscribers.size === 0) return;
        const line = `${JSON.stringify({
            schema: 1,
            type: "watchlist.update",
            id: null,
            payload: watchlistPayload(),
        })}\n`;
        for (const s of watchlistSubscribers) {
            if (s && !s.destroyed && s.writable) {
                try {
                    s.write(line);
                } catch {
                    /* ignore */
                }
            }
        }
    };

    const broadcastVault = () => {
        if (vaultSubscribers.size === 0) return;
        const line = `${JSON.stringify({
            schema: 1,
            type: "vault.update",
            id: null,
            payload: vaultPayload(),
        })}\n`;
        for (const s of vaultSubscribers) {
            if (s && !s.destroyed && s.writable) {
                try {
                    s.write(line);
                } catch {
                    /* ignore */
                }
            }
        }
    };

    const broadcastCasterAccount = () => {
        if (casterAccountSubscribers.size === 0) return;
        const payload = casterAccountPayload();
        if (!payload) return;
        const line = `${JSON.stringify({
            schema: 1,
            type: "casterAccount.update",
            id: null,
            payload,
        })}\n`;
        for (const s of casterAccountSubscribers) {
            if (s && !s.destroyed && s.writable) {
                try {
                    s.write(line);
                } catch {
                    /* ignore */
                }
            }
        }
    };

    const broadcastCasterJournal = () => {
        if (casterJournalSubscribers.size === 0) return;
        const payload = casterJournalPayload();
        if (!payload) return;
        const line = `${JSON.stringify({
            schema: 1,
            type: "casterJournal.update",
            id: null,
            payload,
        })}\n`;
        for (const s of casterJournalSubscribers) {
            if (s && !s.destroyed && s.writable) {
                try {
                    s.write(line);
                } catch {
                    /* ignore */
                }
            }
        }
    };

    /** @param {Record<string, unknown>} fillPayload */
    const broadcastCasterTicket = (fillPayload) => {
        if (!fillPayload || typeof fillPayload !== "object" || casterTicketSubscribers.size === 0) {
            return;
        }
        const line = `${JSON.stringify({
            schema: 1,
            type: "casterTicket.notify",
            id: null,
            payload: fillPayload,
        })}\n`;
        for (const s of casterTicketSubscribers) {
            if (s && !s.destroyed && s.writable) {
                try {
                    s.write(line);
                } catch {
                    /* ignore */
                }
            }
        }
    };

    const broadcastAdminStatus = () => {
        if (adminSubscribers.size === 0) return;
        const payload = connectionsSnapshot(ctx, host, port);
        const line = `${JSON.stringify({
            schema: 1,
            type: "admin.update",
            id: null,
            payload,
        })}\n`;
        for (const s of adminSubscribers) {
            if (s && !s.destroyed && s.writable) {
                try {
                    s.write(line);
                } catch {
                    /* ignore */
                }
            }
        }
    };

    const ctx = {
        watchlistSubscribers,
        vaultSubscribers,
        casterAccountSubscribers,
        casterJournalSubscribers,
        casterTicketSubscribers,
        adminSubscribers,
        host,
        port,
        getMonitorPublisher: () => monitorPublisher,
        setMonitorPublisher: (s) => {
            monitorPublisher = s;
        },
        broadcastWatchlist,
        broadcastVault,
        broadcastCasterAccount,
        broadcastCasterJournal,
        broadcastCasterTicket,
        notifyConnectionsChanged: broadcastAdminStatus,
    };

    const onMonitorPublisherData = (publisherSocket, chunk) => {
        monitorBuf += chunk;
        const parts = monitorBuf.split("\n");
        monitorBuf = parts.pop() || "";
        for (const line of parts) {
            const trimmed = line.trim();
            if (!trimmed) continue;
            const msg = safeParseLine(trimmed);
            if (!msg) continue;
            const reqId = msg.id != null ? String(msg.id) : null;
            if (reqId && pendingByRequestId.has(reqId)) {
                const client = pendingByRequestId.get(reqId);
                pendingByRequestId.delete(reqId);
                if (client && !client.destroyed && client.writable) {
                    client.write(`${trimmed}\n`);
                }
                if (
                    msg.type === "watchlist.response" &&
                    msg.payload &&
                    typeof msg.payload === "object"
                ) {
                    mergeWatchlistPublish(msg.payload);
                    broadcastWatchlist();
                }
                continue;
            }
            const type = msg.type;
            if (type && MONITOR_PUBLISH_TYPES.has(type)) {
                handleMessage(publisherSocket, msg, ctx);
            }
        }
    };

    const server = net.createServer((socket) => {
        socket.setEncoding("utf8");
        let buf = "";

        const cleanup = () => {
            const wasTracked =
                watchlistSubscribers.has(socket) ||
                socket === monitorPublisher ||
                adminSubscribers.has(socket);
            watchlistSubscribers.delete(socket);
            vaultSubscribers.delete(socket);
            casterAccountSubscribers.delete(socket);
            casterJournalSubscribers.delete(socket);
            casterTicketSubscribers.delete(socket);
            adminSubscribers.delete(socket);
            if (socket === monitorPublisher) {
                monitorPublisher = null;
                monitorBuf = "";
                onMonitorPublisherDisconnect(ctx);
            } else if (wasTracked) {
                broadcastAdminStatus();
            }
            for (const [k, v] of pendingByRequestId) {
                if (v === socket) pendingByRequestId.delete(k);
            }
        };

        socket.on("close", cleanup);
        socket.on("error", cleanup);

        socket.on("data", (chunk) => {
            if (socket === monitorPublisher) {
                onMonitorPublisherData(socket, chunk);
                return;
            }

            buf += chunk;
            const parts = buf.split("\n");
            buf = parts.pop() || "";
            for (const line of parts) {
                const msg = safeParseLine(line);
                if (!msg) continue;

                const type = msg.type;
                const id = msg.id != null ? String(msg.id) : null;
                if (FORWARD_TO_MONITOR.has(type)) {
                    const pub = monitorPublisher;
                    if (!pub || pub.destroyed || !pub.writable) {
                        writeJsonLine(
                            socket,
                            {
                                schema: 1,
                                type: "error",
                                id,
                                payload: {
                                    code: "monitor_unavailable",
                                    message: "Arcane Monitor is not connected",
                                },
                            },
                        );
                        continue;
                    }
                    if (id) pendingByRequestId.set(id, socket);
                    pub.write(`${JSON.stringify(msg)}\n`);
                    continue;
                }

                handleMessage(socket, msg, ctx);
            }
        });
    });

    return new Promise((resolve, reject) => {
        server.on("error", reject);
        server.listen(port, host, () => {
            log(`[arcane-bridge] listening on ${host}:${port}`);
            resolve(server);
        });
    });
}
