/** In-memory hub state — Monitor publishes; Caster publishes feed focus. */

const state = {
    watchlist: {
        symbols: [],
        manualFocusSymbol: null,
        feedFocusSymbol: null,
        activeStocks: [],
        activeTicker: null,
        activeHeroModeEnabled: true,
        publisherUserId: null,
    },
    vault: {
        symbols: [],
        publisherUserId: null,
    },
    casterAccount: null,
    casterJournal: null,
    /** Guilds login feed config (user id + guild room ids for Ticket auto-posts). */
    guildsFeed: null,
    monitorToken: null,
};

function isInvalidSymbolPlaceholder(s) {
    return s === "NULL" || s === "UNDEFINED" || s === "NONE" || s === "NIL";
}

function normalizeSymbol(raw) {
    if (raw == null) return null;
    const t = String(raw)
        .trim()
        .toUpperCase();
    if (!t || isInvalidSymbolPlaceholder(t)) return null;
    return t;
}

function normalizeSymbols(arr) {
    if (!Array.isArray(arr)) return [];
    const out = [];
    const seen = new Set();
    for (const raw of arr) {
        const s = normalizeSymbol(raw);
        if (!s || seen.has(s)) continue;
        seen.add(s);
        out.push(s);
    }
    return out;
}

export function watchlistPayload() {
    const w = state.watchlist;
    const scrub = (sym) => {
        const s = normalizeSymbol(sym);
        return s;
    };
    const payload = {
        symbols: normalizeSymbols(w.symbols),
        manualFocusSymbol: scrub(w.manualFocusSymbol),
        activeStocks: w.activeStocks.map((x) => ({ ...x })),
        activeTicker: scrub(w.activeTicker),
        activeHeroModeEnabled: w.activeHeroModeEnabled === true,
    };
    const feed = scrub(w.feedFocusSymbol);
    if (feed) {
        payload.feedFocusSymbol = feed;
    }
    if (w.publisherUserId) {
        payload.publisherUserId = w.publisherUserId;
    }
    return payload;
}

export function vaultPayload() {
    const v = state.vault;
    const payload = { symbols: [...v.symbols] };
    if (v.publisherUserId) {
        payload.publisherUserId = v.publisherUserId;
    }
    return payload;
}

export function casterAccountPayload() {
    const a = state.casterAccount;
    if (!a || typeof a !== "object") {
        return null;
    }
    return { ...a };
}

export function mergeCasterAccountPublish(payload) {
    if (!payload || typeof payload !== "object") return;
    state.casterAccount = { ...payload };
}

export function clearCasterAccount() {
    state.casterAccount = null;
}

export function casterJournalPayload() {
    const j = state.casterJournal;
    if (!j || typeof j !== "object") {
        return null;
    }
    return { ...j };
}

export function mergeCasterJournalPublish(payload) {
    if (!payload || typeof payload !== "object") return;
    state.casterJournal = { ...payload };
}

export function clearCasterJournal() {
    state.casterJournal = null;
}

export function guildsFeedPayload() {
    const g = state.guildsFeed;
    if (!g || typeof g !== "object") {
        return null;
    }
    return { ...g };
}

export function mergeGuildsFeedPublish(payload) {
    if (!payload || typeof payload !== "object") return;
    state.guildsFeed = { ...payload };
}

export function clearGuildsFeed() {
    state.guildsFeed = null;
}

export function mergeWatchlistPublish(payload) {
    if (!payload || typeof payload !== "object") return;
    const w = state.watchlist;
    if (Array.isArray(payload.symbols)) {
        w.symbols = normalizeSymbols(payload.symbols);
    }
    if (Object.prototype.hasOwnProperty.call(payload, "manualFocusSymbol")) {
        const raw = payload.manualFocusSymbol;
        w.manualFocusSymbol =
            raw != null && String(raw).trim() !== "" ? normalizeSymbol(raw) : null;
    }
    if (Object.prototype.hasOwnProperty.call(payload, "feedFocusSymbol")) {
        const raw = payload.feedFocusSymbol;
        w.feedFocusSymbol =
            raw != null && String(raw).trim() !== "" ? normalizeSymbol(raw) : null;
    }
    if (Array.isArray(payload.activeStocks)) {
        w.activeStocks = payload.activeStocks;
    }
    if (Object.prototype.hasOwnProperty.call(payload, "activeTicker")) {
        const raw = payload.activeTicker;
        w.activeTicker = raw != null && String(raw).trim() !== "" ? normalizeSymbol(raw) : null;
    }
    if (Object.prototype.hasOwnProperty.call(payload, "activeHeroModeEnabled")) {
        w.activeHeroModeEnabled = payload.activeHeroModeEnabled === true;
    }
    if (payload.publisherUserId != null && String(payload.publisherUserId).trim() !== "") {
        w.publisherUserId = String(payload.publisherUserId).trim();
    }
}

export function mergeVaultPublish(payload) {
    if (!payload || typeof payload !== "object") return;
    if (Array.isArray(payload.symbols)) {
        state.vault.symbols = normalizeSymbols(payload.symbols);
    }
    if (payload.publisherUserId != null && String(payload.publisherUserId).trim() !== "") {
        state.vault.publisherUserId = String(payload.publisherUserId).trim();
    }
}

export function setFeedFocusSymbol(sym) {
    state.watchlist.feedFocusSymbol = sym;
}

export function setMonitorToken(token) {
    state.monitorToken =
        token != null && String(token).trim() !== "" ? String(token).trim() : null;
}

export function clearMonitorPublisherState() {
    state.watchlist = {
        symbols: [],
        manualFocusSymbol: null,
        feedFocusSymbol: null,
        activeStocks: [],
        activeTicker: null,
        activeHeroModeEnabled: true,
        publisherUserId: null,
    };
    state.vault = { symbols: [], publisherUserId: null };
    state.casterAccount = null;
    state.casterJournal = null;
    state.guildsFeed = null;
    state.monitorToken = null;
}

export function getMonitorToken() {
    return state.monitorToken;
}
