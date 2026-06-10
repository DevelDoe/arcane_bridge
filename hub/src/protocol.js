import {
    connectionsSnapshot,
    inferRoleFromClientId,
    tagSocket,
} from "./connections.js";
import {
    casterAccountPayload,
    casterJournalPayload,
    clearMonitorPublisherState,
    getMonitorToken,
    mergeCasterAccountPublish,
    mergeCasterJournalPublish,
    mergeGuildsFeedPublish,
    guildsFeedPayload,
    mergeVaultPublish,
    mergeWatchlistPublish,
    setFeedFocusSymbol,
    setMonitorToken,
    vaultPayload,
    watchlistPayload,
} from "./state.js";

function writeJsonLine(socket, obj) {
    if (!socket || socket.destroyed || !socket.writable) return;
    socket.write(`${JSON.stringify(obj)}\n`);
}

function errorLine(id, code, message) {
    return {
        schema: 1,
        type: "error",
        id,
        payload: { code, message },
    };
}

/**
 * @param {import("net").Socket} socket
 * @param {object} msg
 * @param {{
 *   watchlistSubscribers: Set<import("net").Socket>,
 *   vaultSubscribers: Set<import("net").Socket>,
 *   casterAccountSubscribers: Set<import("net").Socket>,
 *   casterJournalSubscribers: Set<import("net").Socket>,
 *   casterTicketSubscribers: Set<import("net").Socket>,
 *   getMonitorPublisher: () => import("net").Socket | null,
 *   setMonitorPublisher: (s: import("net").Socket | null) => void,
 *   broadcastWatchlist: () => void,
 *   broadcastVault: () => void,
 *   broadcastCasterAccount: () => void,
 *   broadcastCasterJournal: () => void,
 *   broadcastCasterTicket: (fill: Record<string, unknown>) => void,
 *   notifyConnectionsChanged: () => void,
 *   host: string,
 *   port: number,
 * }} ctx
 */
export function handleMessage(socket, msg, ctx) {
    if (!msg || typeof msg !== "object") {
        writeJsonLine(socket, errorLine(null, "invalid_envelope", "Expected JSON object"));
        return;
    }

    const schema = msg.schema;
    const type = msg.type;
    const id = msg.id != null ? String(msg.id) : null;

    if (schema !== 1) {
        writeJsonLine(
            socket,
            errorLine(id, "unsupported_schema", "Only schema 1 is supported"),
        );
        return;
    }

    if (type === "hello") {
        writeJsonLine(socket, {
            schema: 1,
            type: "hello.ack",
            id,
            payload: { app: "arcane-bridge", protocol: 1 },
        });
        return;
    }

    if (type === "monitor.register") {
        const old = ctx.getMonitorPublisher();
        if (old && old !== socket) {
            try {
                old.destroy();
            } catch {
                /* ignore */
            }
        }
        ctx.setMonitorPublisher(socket);
        tagSocket(socket, "monitor", id || "monitor-publisher");
        ctx.notifyConnectionsChanged();
        writeJsonLine(socket, {
            schema: 1,
            type: "monitor.register.ack",
            id,
            payload: { ok: true },
        });
        return;
    }

    if (type === "session.request") {
        const token = getMonitorToken();
        if (token) {
            writeJsonLine(socket, {
                schema: 1,
                type: "session.response",
                id,
                payload: { ok: true, token },
            });
        } else {
            writeJsonLine(socket, {
                schema: 1,
                type: "session.response",
                id,
                payload: { ok: false, reason: "not_logged_in" },
            });
        }
        return;
    }

    if (type === "session.publish") {
        const raw = msg.payload?.token;
        setMonitorToken(raw != null && String(raw).trim() !== "" ? String(raw).trim() : null);
        return;
    }

    if (type === "watchlist.publish") {
        mergeWatchlistPublish(msg.payload);
        ctx.broadcastWatchlist();
        writeJsonLine(socket, {
            schema: 1,
            type: "watchlist.publish.ack",
            id,
            payload: { ok: true },
        });
        return;
    }

    if (type === "vault.publish") {
        mergeVaultPublish(msg.payload);
        ctx.broadcastVault();
        writeJsonLine(socket, {
            schema: 1,
            type: "vault.publish.ack",
            id,
            payload: { ok: true },
        });
        return;
    }

    if (type === "feedFocus.publish") {
        const raw = msg.payload && msg.payload.symbol;
        const sym =
            raw != null && String(raw).trim() !== ""
                ? String(raw).trim().toUpperCase()
                : null;
        setFeedFocusSymbol(sym);
        ctx.broadcastWatchlist();
        writeJsonLine(socket, {
            schema: 1,
            type: "feedFocus.ack",
            id,
            payload: { ok: true, symbol: sym },
        });
        return;
    }

    if (type === "casterAccount.publish") {
        mergeCasterAccountPublish(msg.payload);
        ctx.broadcastCasterAccount();
        writeJsonLine(socket, {
            schema: 1,
            type: "casterAccount.ack",
            id,
            payload: { ok: true },
        });
        return;
    }

    if (type === "casterAccount.subscribe") {
        ctx.casterAccountSubscribers.add(socket);
        const payload = casterAccountPayload();
        if (payload) {
            writeJsonLine(socket, {
                schema: 1,
                type: "casterAccount.snapshot",
                id,
                payload,
            });
        }
        return;
    }

    if (type === "casterAccount.request") {
        const payload = casterAccountPayload();
        writeJsonLine(socket, {
            schema: 1,
            type: "casterAccount.response",
            id,
            payload: payload ?? {},
        });
        return;
    }

    if (type === "casterJournal.publish") {
        mergeCasterJournalPublish(msg.payload);
        ctx.broadcastCasterJournal();
        writeJsonLine(socket, {
            schema: 1,
            type: "casterJournal.ack",
            id,
            payload: { ok: true },
        });
        return;
    }

    if (type === "casterJournal.subscribe") {
        ctx.casterJournalSubscribers.add(socket);
        const payload = casterJournalPayload();
        if (payload) {
            writeJsonLine(socket, {
                schema: 1,
                type: "casterJournal.snapshot",
                id,
                payload,
            });
        }
        return;
    }

    if (type === "casterJournal.request") {
        const payload = casterJournalPayload();
        writeJsonLine(socket, {
            schema: 1,
            type: "casterJournal.response",
            id,
            payload: payload ?? {},
        });
        return;
    }

    if (type === "casterTicket.notify") {
        const fill = msg.payload;
        if (fill && typeof fill === "object") {
            ctx.broadcastCasterTicket(fill);
        }
        writeJsonLine(socket, {
            schema: 1,
            type: "casterTicket.ack",
            id,
            payload: { ok: true },
        });
        return;
    }

    if (type === "casterTicket.subscribe") {
        ctx.casterTicketSubscribers.add(socket);
        return;
    }

    if (type === "casterBlowup.notify") {
        const event = msg.payload;
        if (event && typeof event === "object") {
            ctx.broadcastCasterBlowup(event);
        }
        writeJsonLine(socket, {
            schema: 1,
            type: "casterBlowup.ack",
            id,
            payload: { ok: true },
        });
        return;
    }

    if (type === "casterBlowup.subscribe") {
        ctx.casterBlowupSubscribers.add(socket);
        return;
    }

    if (type === "guildsFeed.publish") {
        mergeGuildsFeedPublish(msg.payload);
        writeJsonLine(socket, {
            schema: 1,
            type: "guildsFeed.ack",
            id,
            payload: { ok: true },
        });
        return;
    }

    if (type === "guildsFeed.request") {
        const payload = guildsFeedPayload();
        writeJsonLine(socket, {
            schema: 1,
            type: "guildsFeed.response",
            id,
            payload: payload ?? {},
        });
        return;
    }

    if (type === "watchlist.subscribe") {
        ctx.watchlistSubscribers.add(socket);
        const role = inferRoleFromClientId(id);
        tagSocket(socket, role, id || "watchlist-subscriber");
        ctx.notifyConnectionsChanged();
        writeJsonLine(socket, {
            schema: 1,
            type: "watchlist.snapshot",
            id,
            payload: watchlistPayload(),
        });
        return;
    }

    if (type === "admin.subscribe") {
        ctx.adminSubscribers.add(socket);
        tagSocket(socket, "bridge_app", id || "bridge-admin");
        ctx.notifyConnectionsChanged();
        writeJsonLine(socket, {
            schema: 1,
            type: "admin.snapshot",
            id,
            payload: connectionsSnapshot(ctx, ctx.host, ctx.port),
        });
        return;
    }

    if (type === "admin.request") {
        writeJsonLine(socket, {
            schema: 1,
            type: "admin.response",
            id,
            payload: connectionsSnapshot(ctx, ctx.host, ctx.port),
        });
        return;
    }

    if (type === "watchlist.request") {
        writeJsonLine(socket, {
            schema: 1,
            type: "watchlist.response",
            id,
            payload: watchlistPayload(),
        });
        return;
    }

    if (type === "vault.subscribe") {
        ctx.vaultSubscribers.add(socket);
        writeJsonLine(socket, {
            schema: 1,
            type: "vault.snapshot",
            id,
            payload: vaultPayload(),
        });
        return;
    }

    if (type === "vault.request") {
        writeJsonLine(socket, {
            schema: 1,
            type: "vault.response",
            id,
            payload: vaultPayload(),
        });
        return;
    }

    writeJsonLine(socket, errorLine(id, "unknown_type", `Unknown type: ${type}`));
}

export function onMonitorPublisherDisconnect(ctx) {
    ctx.setMonitorPublisher(null);
    clearMonitorPublisherState();
    ctx.broadcastWatchlist();
    ctx.broadcastVault();
    ctx.notifyConnectionsChanged();
}
