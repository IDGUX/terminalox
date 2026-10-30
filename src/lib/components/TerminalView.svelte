<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Terminal, type IDisposable } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import "@xterm/xterm/css/xterm.css";
  import { api } from "$lib/api";
  import { isAppShortcut } from "$lib/shortcuts";
  import { app } from "$lib/stores/app.svelte";
  import type { Server, SessionClosedEvent, SessionDataEvent } from "$lib/types";

  let {
    server,
    onSplit,
    onClose,
    canSplit = false,
    canClose = false,
  }: {
    server: Server;
    onSplit?: () => void;
    onClose?: () => void;
    canSplit?: boolean;
    canClose?: boolean;
  } = $props();

  let host: HTMLDivElement;
  let term: Terminal | null = null;
  let fit: FitAddon | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let sessionId: string | null = null;
  let unlistenData: UnlistenFn | null = null;
  let unlistenClosed: UnlistenFn | null = null;
  let onDataDisposable: IDisposable | null = null;
  let onResizeDisposable: IDisposable | null = null;
  let status = $state<"connecting" | "connected" | "closed" | "error">("connecting");
  let statusDetail = $state<string>("");
  let passwordPrompt = $state<string | null>(null);
  let passwordInput = $state("");

  function decode(b64: string): Uint8Array {
    const bin = atob(b64);
    const arr = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
    return arr;
  }

  function encode(bytes: Uint8Array): string {
    let bin = "";
    for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
    return btoa(bin);
  }

  async function teardownSession() {
    unlistenData?.();
    unlistenData = null;
    unlistenClosed?.();
    unlistenClosed = null;
    onDataDisposable?.dispose();
    onDataDisposable = null;
    onResizeDisposable?.dispose();
    onResizeDisposable = null;
    const sid = sessionId;
    sessionId = null;
    if (sid) {
      try {
        await api.closeSession(sid);
      } catch {}
    }
  }

  async function start(password: string | null) {
    status = "connecting";
    statusDetail = "";

    if (!term) return;
    term.clear();
    const cols = term.cols;
    const rows = term.rows;

    try {
      sessionId = await api.openSession(server.id, password, cols, rows);
    } catch (e) {
      const msg = String(e);
      if (server.auth.mode === "password" && !password) {
        passwordPrompt = "Passwort erforderlich";
        status = "error";
        statusDetail = "";
        return;
      }
      status = "error";
      statusDetail = msg;
      return;
    }

    app.activeSessionId = sessionId;
    app.markUsed(server.id);
    status = "connected";
    passwordPrompt = null;

    const sid = sessionId;
    unlistenData = await listen<SessionDataEvent>(`session:${sid}:data`, (ev) => {
      term?.write(decode(ev.payload.dataB64));
    });
    unlistenClosed = await listen<SessionClosedEvent>(`session:${sid}:closed`, (ev) => {
      status = "closed";
      statusDetail = ev.payload.error
        ? `Fehler: ${ev.payload.error}`
        : ev.payload.exitCode !== null && ev.payload.exitCode !== undefined
          ? `Exit ${ev.payload.exitCode}`
          : "Verbindung beendet";
    });

    onDataDisposable = term.onData((str) => {
      if (!sessionId) return;
      const bytes = new TextEncoder().encode(str);
      api.writeSession(sessionId, encode(bytes)).catch(() => {});
    });

    onResizeDisposable = term.onResize(({ cols, rows }) => {
      if (!sessionId) return;
      api.resizeSession(sessionId, cols, rows).catch(() => {});
    });
  }

  async function reconnect() {
    await teardownSession();
    if (server.auth.mode === "password") {
      passwordPrompt = "Passwort erforderlich";
      status = "connecting";
      statusDetail = "";
    } else {
      await start(null);
    }
  }

  async function disconnect() {
    await teardownSession();
    status = "closed";
    statusDetail = "Manuell getrennt";
  }

  async function submitPassword(e: Event) {
    e.preventDefault();
    const p = passwordInput;
    passwordInput = "";
    await start(p);
  }

  function resize() {
    fit?.fit();
  }

  onMount(async () => {
    term = new Terminal({
      fontFamily: "'JetBrains Mono', 'Menlo', monospace",
      fontSize: 13,
      theme: {
        background: "#0d1017",
        foreground: "#d1d5db",
        cursor: "#60a5fa",
      },
      cursorBlink: true,
      scrollback: 5000,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    // ⌘K (Strg+Shift+K) leert Bildschirm + Scrollback wie in Terminal.app/iTerm;
    // die aktuelle Prompt-Zeile bleibt stehen.
    term.attachCustomKeyEventHandler((e) => {
      if (!isAppShortcut(e, "k")) return true;
      if (e.type === "keydown") term?.clear();
      return false;
    });
    term.open(host);
    fit.fit();
    window.addEventListener("resize", resize);
    resizeObserver = new ResizeObserver(() => fit?.fit());
    resizeObserver.observe(host);

    if (server.auth.mode === "password") {
      passwordPrompt = "Passwort erforderlich";
    } else {
      await start(null);
    }
  });

  onDestroy(() => {
    window.removeEventListener("resize", resize);
    resizeObserver?.disconnect();
    resizeObserver = null;
    teardownSession();
    term?.dispose();
  });
</script>

<div class="container">
  <div class="header">
    <div class="title">
      <strong>{server.name}</strong>
      <span class="muted">{server.user}@{server.host}{server.port !== 22 ? `:${server.port}` : ""}</span>
    </div>
    <div class="actions">
      <button
        type="button"
        class="hbtn"
        onclick={reconnect}
        title="Erneut verbinden"
        aria-label="Erneut verbinden"
      >↻</button>
      <button
        type="button"
        class="hbtn"
        onclick={disconnect}
        disabled={status !== "connected" && status !== "connecting"}
        title="Verbindung beenden"
        aria-label="Verbindung beenden"
      >⏻</button>
      {#if canSplit && onSplit}
        <button
          type="button"
          class="hbtn"
          onclick={onSplit}
          title="Neues Terminal (Split)"
          aria-label="Neues Terminal"
        >＋</button>
      {/if}
      {#if canClose && onClose}
        <button
          type="button"
          class="hbtn close"
          onclick={onClose}
          title="Diesen Bereich schließen"
          aria-label="Bereich schließen"
        >✕</button>
      {/if}
      <div
        class="status"
        class:ok={status === "connected"}
        class:bad={status === "error" || status === "closed"}
      >
        {status}{statusDetail ? ` — ${statusDetail}` : ""}
      </div>
    </div>
  </div>

  <div class="term" bind:this={host}></div>

  {#if passwordPrompt}
    <div class="pw-overlay">
      <form class="pw" onsubmit={submitPassword}>
        <label>
          {passwordPrompt}
          <input
            type="password"
            bind:value={passwordInput}
            autofocus
            placeholder="SSH-Passwort"
          />
        </label>
        <div class="pw-buttons">
          <button type="submit" class="primary">Verbinden</button>
        </div>
        <p class="hint">
          Tipp: Nach der Verbindung kannst du links „SSH-Key deployen" klicken —
          danach brauchst du kein Passwort mehr.
        </p>
      </form>
    </div>
  {/if}
</div>

<style>
  .container {
    flex: 1;
    display: flex;
    flex-direction: column;
    position: relative;
    background: #0d1017;
    min-height: 0;
    min-width: 0;
    border: 1px solid #1f232b;
    border-radius: 4px;
    overflow: hidden;
  }
  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 10px;
    background: #151923;
    border-bottom: 1px solid #2a2e37;
    color: #d1d5db;
    font-size: 13px;
    gap: 8px;
  }
  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted { color: #6b7280; margin-left: 8px; font-size: 12px; }
  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .hbtn {
    background: transparent;
    border: 1px solid #2a2e37;
    color: #9ca3af;
    width: 26px;
    height: 26px;
    border-radius: 5px;
    cursor: pointer;
    font-size: 13px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    padding: 0;
  }
  .hbtn:hover:not(:disabled) {
    background: #1f232b;
    color: #e5e7eb;
    border-color: #3a3f4a;
  }
  .hbtn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .hbtn.close:hover { color: #f87171; border-color: #4a2222; }
  .status {
    font-size: 11px;
    color: #9ca3af;
    padding: 2px 8px;
    border-radius: 10px;
    background: #1f232b;
    margin-left: 4px;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status.ok { color: #34d399; background: #052e26; }
  .status.bad { color: #f87171; background: #2d1212; }

  .term {
    flex: 1;
    padding: 4px;
    min-height: 0;
  }

  .pw-overlay {
    position: absolute;
    inset: 0;
    background: rgba(0,0,0,0.55);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .pw {
    background: #1f232b;
    padding: 18px;
    border-radius: 10px;
    border: 1px solid #2a2e37;
    width: 360px;
    color: #e5e7eb;
  }
  .pw label { font-size: 12px; color: #9ca3af; display: block; }
  .pw input {
    width: 100%;
    padding: 8px 10px;
    background: #11141a;
    border: 1px solid #2a2e37;
    color: #e5e7eb;
    border-radius: 6px;
    margin-top: 6px;
    box-sizing: border-box;
    font-size: 14px;
  }
  .pw-buttons { margin-top: 12px; display: flex; justify-content: flex-end; }
  .primary {
    padding: 8px 14px;
    border-radius: 6px;
    border: 1px solid #2563eb;
    background: #2563eb;
    color: white;
    cursor: pointer;
    font-size: 13px;
  }
  .hint { font-size: 11px; color: #6b7280; margin-top: 10px; line-height: 1.4; }
</style>
