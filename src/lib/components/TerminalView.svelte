<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Terminal, type IDisposable } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import "@xterm/xterm/css/xterm.css";
  import { api } from "$lib/api";
  import { isAppShortcut } from "$lib/shortcuts";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { Server, SessionClosedEvent } from "$lib/types";

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
  /** Bumped per connection attempt; late output of an old session is dropped. */
  let generation = 0;
  let onDataDisposable: IDisposable | null = null;
  let onResizeDisposable: IDisposable | null = null;
  let status = $state<"connecting" | "connected" | "closed" | "error">("connecting");
  let statusDetail = $state<string>("");

  const encoder = new TextEncoder();

  function encode(bytes: Uint8Array): string {
    let bin = "";
    for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
    return btoa(bin);
  }

  async function teardownSession() {
    generation++;
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

  function onClosed(ev: SessionClosedEvent) {
    status = "closed";
    statusDetail = ev.error
      ? `Fehler: ${ev.error}`
      : ev.exitCode !== null && ev.exitCode !== undefined
        ? `Exit ${ev.exitCode}`
        : "Verbindung beendet";
  }

  async function start() {
    status = "connecting";
    statusDetail = "";

    if (!term) return;
    term.clear();
    const gen = ++generation;

    let sid: string;
    try {
      // Passwörter/Passphrasen fragt der globale Dialog ab (siehe api.invoke).
      sid = await api.openSession(
        server.id,
        term.cols,
        term.rows,
        (bytes) => gen === generation && term?.write(bytes),
        (ev) => gen === generation && onClosed(ev),
      );
    } catch (e) {
      if (gen !== generation) return;
      status = "error";
      statusDetail = String(e);
      return;
    }
    if (gen !== generation) {
      // Torn down while connecting.
      api.closeSession(sid).catch(() => {});
      return;
    }
    sessionId = sid;

    app.activeSessionId = sessionId;
    app.markUsed(server.id);
    status = "connected";

    onDataDisposable = term.onData((str) => {
      if (!sessionId) return;
      api.writeSession(sessionId, encode(encoder.encode(str))).catch(() => {});
    });

    onResizeDisposable = term.onResize(({ cols, rows }) => {
      if (!sessionId) return;
      api.resizeSession(sessionId, cols, rows).catch(() => {});
    });
  }

  async function reconnect() {
    await teardownSession();
    await start();
  }

  async function disconnect() {
    await teardownSession();
    status = "closed";
    statusDetail = "Manuell getrennt";
  }

  function resize() {
    fit?.fit();
  }

  /** Links öffnen im System-Browser, nie im App-Fenster. */
  function openLink(_ev: MouseEvent, uri: string) {
    if (!/^https?:\/\//i.test(uri)) return;
    openUrl(uri).catch((e) => toast.error(`Link konnte nicht geöffnet werden: ${e}`));
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
    term.loadAddon(new WebLinksAddon(openLink));
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

    await start();
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

</style>
