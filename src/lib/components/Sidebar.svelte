<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { HardenReadiness, Server, Uuid } from "$lib/types";

  let {
    onAdd,
    onEdit,
    onDeploy,
    onHarden,
    onDelete,
    onExport,
    onImport,
  }: {
    onAdd: () => void;
    onEdit: (s: Server) => void;
    onDeploy: (s: Server) => void;
    onHarden: (s: Server, r: HardenReadiness & { status: "ready" }) => void;
    onDelete: (s: Server) => void;
    onExport: () => void;
    onImport: () => void;
  } = $props();

  type ReadinessState =
    | { kind: "idle" }
    | { kind: "loading" }
    | { kind: "done"; value: HardenReadiness }
    | { kind: "error"; message: string };

  let readinessByServer = $state<Record<Uuid, ReadinessState>>({});

  function select(s: Server) {
    app.selectedServerId = s.id;
    ensureReadiness(s);
  }

  function ensureReadiness(s: Server) {
    if (s.auth.mode !== "key" || s.hardened) return;
    const cur = readinessByServer[s.id];
    if (cur && cur.kind !== "error") return;
    readinessByServer = { ...readinessByServer, [s.id]: { kind: "loading" } };
    api
      .checkHardenReadiness(s.id)
      .then((value) => {
        readinessByServer = { ...readinessByServer, [s.id]: { kind: "done", value } };
      })
      .catch((e) => {
        readinessByServer = {
          ...readinessByServer,
          [s.id]: { kind: "error", message: String(e) },
        };
      });
  }

  function currentReadiness(s: Server | null): ReadinessState {
    if (!s) return { kind: "idle" };
    return readinessByServer[s.id] ?? { kind: "idle" };
  }

  function hardenButtonTitle(r: ReadinessState): string {
    if (r.kind === "loading") return "Prüfe, ob Härten möglich ist …";
    if (r.kind === "error") return `Konnte Readiness nicht prüfen: ${r.message}`;
    if (r.kind === "done" && r.value.status === "unavailable") return r.value.reason;
    if (r.kind === "done" && r.value.status === "ready") {
      if (r.value.method === "root") return "Härten möglich (als root eingeloggt)";
      if (r.value.method === "sudo_nopasswd") return "Härten möglich (passwortloses sudo)";
      return "Härten möglich (sudo, sudo-Passwort wird einmalig abgefragt)";
    }
    return "Passwort-Login serverseitig deaktivieren";
  }

  function hardenButtonLabel(r: ReadinessState): string {
    if (r.kind === "loading") return "Prüfe …";
    if (r.kind === "done" && r.value.status === "ready") return "Server härten";
    return "Härten nicht verfügbar";
  }

  function onHardenClick(s: Server) {
    const r = currentReadiness(s);
    if (r.kind === "done" && r.value.status === "ready") {
      onHarden(s, r.value);
    } else if (r.kind === "done" && r.value.status === "unavailable") {
      toast.error(r.value.reason);
    } else if (r.kind === "error") {
      toast.error(r.message);
    }
  }

  function showReadinessInfo(s: Server) {
    const r = currentReadiness(s);
    if (r.kind === "done" && r.value.status === "unavailable") {
      toast.error(r.value.reason);
    } else if (r.kind === "error") {
      toast.error(r.message);
    }
  }

  type AuthState = "password" | "key-unhardened" | "hardened";

  function authState(s: Server): AuthState {
    if (s.hardened) return "hardened";
    if (s.auth.mode === "key") return "key-unhardened";
    return "password";
  }

  function authTitle(s: Server): string {
    const st = authState(s);
    if (st === "hardened") return "Gehärtet: nur Key-Login möglich, Passwort-Login aus";
    if (st === "key-unhardened")
      return 'Key-Auth aktiv — aber Passwort-Login ist auf dem Server NOCH möglich. „Server härten" empfohlen.';
    return "Passwort-Auth";
  }
</script>

<aside class="sidebar">
  <header>
    <h2>terminalox</h2>
    <button class="add" onclick={onAdd} title="Neuer Server">+</button>
  </header>

  <ul class="servers">
    {#if app.servers.length === 0}
      <li class="empty">Noch keine Server. Klick auf <strong>+</strong>.</li>
    {/if}
    {#each app.servers as s (s.id)}
      <li
        class="server"
        class:active={app.selectedServerId === s.id}
        onclick={() => select(s)}
      >
        <div class="row">
          <span class="auth auth-{authState(s)}" title={authTitle(s)}>
            {#if authState(s) === "hardened"}🛡
            {:else if authState(s) === "key-unhardened"}🔑<span class="warn-dot" aria-hidden="true">!</span>
            {:else}🔓{/if}
          </span>
          <span class="name">{s.name}</span>
        </div>
        <div class="meta">{s.user}@{s.host}{s.port !== 22 ? `:${s.port}` : ""}</div>
      </li>
    {/each}
  </ul>

  {#if app.selectedServer}
    {@const sel = app.selectedServer}
    {@const rd = currentReadiness(sel)}
    <div class="actions">
      <button onclick={() => onDeploy(sel)}>SSH-Key deployen</button>
      {#if sel.auth.mode === "key" && !sel.hardened}
        {@const ready = rd.kind === "done" && rd.value.status === "ready"}
        {@const showInfo =
          (rd.kind === "done" && rd.value.status === "unavailable") || rd.kind === "error"}
        <div class="harden-row">
          <button
            class="warn harden-btn"
            disabled={!ready}
            onclick={() => onHardenClick(sel)}
            title={hardenButtonTitle(rd)}
          >
            🛡 {hardenButtonLabel(rd)}
          </button>
          {#if showInfo}
            <button
              type="button"
              class="info-btn"
              onclick={() => showReadinessInfo(sel)}
              title="Warum ist Härten nicht möglich?"
              aria-label="Details anzeigen"
            >
              ⓘ
            </button>
          {/if}
        </div>
      {/if}
      <button onclick={() => onEdit(sel)}>
        Bearbeiten
      </button>
      <button class="danger" onclick={() => onDelete(sel)}>
        Löschen…
      </button>
    </div>
  {/if}

  <footer>
    <button
      class="ghost"
      onclick={async () => {
        try {
          const p = await app.syncSshConfig();
          toast.success(`~/.ssh/config geschrieben (${p})`);
        } catch (e) {
          toast.error(String(e));
        }
      }}
    >
      ~/.ssh/config schreiben
    </button>
    <div class="backup-row">
      <button class="ghost-half" onclick={onExport} title="Backup exportieren">↑ Export</button>
      <button class="ghost-half" onclick={onImport} title="Backup importieren">↓ Import</button>
    </div>
  </footer>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 260px;
    min-width: 260px;
    background: #1a1d23;
    border-right: 1px solid #2a2e37;
    color: #d1d5db;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    border-bottom: 1px solid #2a2e37;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  button.add {
    background: transparent;
    border: 1px solid #3a3f4b;
    color: #d1d5db;
    border-radius: 6px;
    width: 28px;
    height: 28px;
    cursor: pointer;
    font-size: 18px;
  }
  button.add:hover { background: #2a2e37; }

  ul.servers {
    list-style: none;
    padding: 6px;
    margin: 0;
    flex: 1;
    overflow-y: auto;
  }
  li.server {
    padding: 8px 10px;
    border-radius: 6px;
    cursor: pointer;
    margin-bottom: 2px;
  }
  li.server:hover { background: #232732; }
  li.server.active { background: #2d3440; }
  li.empty { color: #6b7280; padding: 12px; font-size: 13px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .auth { font-size: 13px; }
  .name { font-weight: 500; }
  .meta { color: #6b7280; font-size: 12px; margin-top: 2px; margin-left: 22px; }

  .actions { padding: 10px; border-top: 1px solid #2a2e37; display: flex; flex-direction: column; gap: 6px; }
  .actions button {
    background: #2563eb;
    color: white;
    border: none;
    padding: 8px 10px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  .actions button.danger { background: #7f1d1d; }
  .actions button.warn { background: #a16207; }
  .actions button:hover { filter: brightness(1.1); }
  .actions button:disabled { opacity: 0.55; cursor: not-allowed; }
  .actions button:disabled:hover { filter: none; }

  .harden-row { display: flex; gap: 6px; }
  .harden-row .harden-btn { flex: 1; }
  .harden-row .info-btn {
    flex: 0 0 32px;
    background: #2a2e37;
    color: #d1d5db;
    border: 1px solid #3a3f4b;
    padding: 0;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    border-radius: 6px;
  }
  .harden-row .info-btn:hover { background: #3a3f4b; filter: none; }

  .auth {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 20px;
    position: relative;
    font-size: 14px;
    line-height: 1;
  }
  .auth-hardened { filter: drop-shadow(0 0 3px rgba(52, 211, 153, 0.4)); }
  .auth-key-unhardened { position: relative; }
  .warn-dot {
    position: absolute;
    top: -4px;
    right: -4px;
    background: #f59e0b;
    color: #1a1d23;
    font-size: 9px;
    font-weight: 800;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid #1a1d23;
    line-height: 1;
  }

  footer { padding: 10px; border-top: 1px solid #2a2e37; }
  button.ghost {
    width: 100%;
    background: transparent;
    color: #9ca3af;
    border: 1px solid #2a2e37;
    padding: 6px;
    border-radius: 6px;
    font-size: 12px;
    cursor: pointer;
  }
  button.ghost:hover { background: #232732; }

  .backup-row { display: flex; gap: 6px; margin-top: 6px; }
  button.ghost-half {
    flex: 1;
    background: transparent;
    color: #9ca3af;
    border: 1px solid #2a2e37;
    padding: 6px;
    border-radius: 6px;
    font-size: 11px;
    cursor: pointer;
  }
  button.ghost-half:hover { background: #232732; color: #e5e7eb; }
</style>
