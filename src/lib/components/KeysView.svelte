<script lang="ts">
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { KeyInfo, Server } from "$lib/types";
  import ImportKeyDialog from "./ImportKeyDialog.svelte";

  let showImport = $state(false);
  let generating = $state(false);
  let generateName = $state("");
  let showGenerate = $state(false);
  let usage = $state<Server[]>([]);

  function select(k: KeyInfo) {
    app.selectedKeyId = k.id;
    loadUsage(k.id);
  }

  async function loadUsage(keyId: string) {
    try {
      const u = await api.serversUsingKey(keyId);
      usage = u.servers;
    } catch {
      usage = [];
    }
  }

  async function copyPublic(k: KeyInfo) {
    try {
      await writeText(k.public_openssh);
      toast.success(`Public Key von „${k.name}" kopiert`);
    } catch (e) {
      toast.error(`Konnte nicht kopieren: ${e}`);
    }
  }

  async function copyFingerprint(k: KeyInfo) {
    if (!k.fingerprint) return;
    try {
      await writeText(k.fingerprint);
      toast.success("Fingerprint kopiert");
    } catch (e) {
      toast.error(`Konnte nicht kopieren: ${e}`);
    }
  }

  async function copyPath(path: string, label: string) {
    try {
      await writeText(path);
      toast.success(`${label}-Pfad kopiert`);
    } catch (e) {
      toast.error(`Konnte nicht kopieren: ${e}`);
    }
  }

  async function deleteKey(k: KeyInfo) {
    const confirmed = confirm(
      `Key „${k.name}" löschen?\n\nPrivate + Public Key Datei werden entfernt. Das kann nicht rückgängig gemacht werden.`,
    );
    if (!confirmed) return;
    try {
      await app.deleteKey(k.id);
      toast.success(`Key „${k.name}" gelöscht`);
      usage = [];
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function generate(e: Event) {
    e.preventDefault();
    generating = true;
    try {
      const k = await app.generateKey(generateName.trim());
      toast.success(`Key „${k.name}" erzeugt (${k.algorithm})`);
      generateName = "";
      showGenerate = false;
      app.selectedKeyId = k.id;
    } catch (e) {
      toast.error(String(e));
    } finally {
      generating = false;
    }
  }

  $effect(() => {
    if (app.selectedKeyId) {
      loadUsage(app.selectedKeyId);
    }
  });
</script>

<div class="keys-view">
  <aside class="list">
    <header>
      <h2>Keys</h2>
      <div class="actions">
        <button onclick={() => (showGenerate = true)} title="Neuen Key generieren">+ Neu</button>
        <button onclick={() => (showImport = true)} title="Bestehenden Key importieren">↓ Import</button>
      </div>
    </header>

    {#if app.keys.length === 0}
      <div class="empty">
        <p>Noch keine Keys.</p>
        <p class="small">
          „Neu" erzeugt einen Ed25519-Key, „Import" liest einen bestehenden aus
          <code>~/.ssh/</code>.
        </p>
      </div>
    {:else}
      <ul>
        {#each app.keys as k (k.id)}
          <li
            class="item"
            class:active={app.selectedKeyId === k.id}
            onclick={() => select(k)}
          >
            <div class="row-top">
              <span class="algo">{k.algorithm}</span>
              <span class="name">{k.name}</span>
              {#if k.origin_imported}
                <span class="tag imported" title="Importiert">IMP</span>
              {/if}
            </div>
            {#if k.fingerprint}
              <div class="fp">{k.fingerprint}</div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </aside>

  <section class="detail">
    {#if app.selectedKey}
      {@const k = app.selectedKey}
      <header class="detail-header">
        <div>
          <h1>{k.name}</h1>
          <p class="algo-line">{k.algorithm.toUpperCase()} · erstellt {new Date(k.created_at).toLocaleString()}</p>
        </div>
        <button class="primary copy-big" onclick={() => copyPublic(k)}>
          📋 Public Key kopieren
        </button>
      </header>

      <div class="grid">
        <div class="card">
          <div class="card-header">
            <h3>Fingerprint</h3>
            {#if k.fingerprint}
              <button class="ghost-mini" onclick={() => copyFingerprint(k)}>kopieren</button>
            {/if}
          </div>
          <code class="block">{k.fingerprint ?? "—"}</code>
        </div>

        <div class="card">
          <h3>Public Key (OpenSSH)</h3>
          <textarea readonly class="block multi" rows="3">{k.public_openssh}</textarea>
          <p class="muted">
            Zum Hochladen bei Hetzner, DigitalOcean, GitHub, GitLab, Cloud-Init etc.
          </p>
        </div>

        <div class="card">
          <h3>Pfade</h3>
          <div class="path-row">
            <span class="label">Private:</span>
            <code>{k.private_path}</code>
            <button class="ghost-mini" onclick={() => copyPath(k.private_path, 'Private')}>kopieren</button>
          </div>
          <div class="path-row">
            <span class="label">Public:</span>
            <code>{k.public_path}</code>
            <button class="ghost-mini" onclick={() => copyPath(k.public_path, 'Public')}>kopieren</button>
          </div>
          <p class="muted">Perms: 0600 (private), 0644 (public). Kompatibel mit System-<code>ssh</code>.</p>
        </div>

        <div class="card">
          <h3>Verwendet von {usage.length} Server{usage.length === 1 ? "" : "n"}</h3>
          {#if usage.length === 0}
            <p class="muted">Noch nicht deployed. Geh auf Servers → Key deployen, um ihn zu verwenden.</p>
          {:else}
            <ul class="usage-list">
              {#each usage as s (s.id)}
                <li>
                  <span class="status-dot" class:hardened={s.hardened}></span>
                  <strong>{s.name}</strong>
                  <span class="muted">{s.user}@{s.host}</span>
                  {#if s.hardened}<span class="tag hardened">🛡 gehärtet</span>{/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        <div class="card danger">
          <h3>Gefahrenzone</h3>
          {#if usage.length > 0}
            <p class="muted">
              Dieser Key wird noch von {usage.length} Server{usage.length === 1 ? "" : "n"} benutzt.
              Lösche erst die Server (oder stelle sie auf Passwort-Auth um), bevor du den Key löschst.
            </p>
            <button class="danger-btn" disabled>Key löschen</button>
          {:else}
            <p class="muted">
              Entfernt Private + Public Key Datei und den Eintrag im Store. Nicht rückgängig.
            </p>
            <button class="danger-btn" onclick={() => deleteKey(k)}>Key löschen</button>
          {/if}
        </div>
      </div>
    {:else}
      <div class="splash">
        <h1>Keys</h1>
        <p>Wähle links einen Key, um Fingerprint, Public Key, Pfade und verwendende Server zu sehen.</p>
        <p class="small">
          Tipp: „Public Key kopieren" gibt dir genau den String, den du bei Hetzner,
          DigitalOcean, GitHub &amp; Co. in deren SSH-Key-Feld pasten kannst.
        </p>
      </div>
    {/if}
  </section>
</div>

{#if showImport}
  <ImportKeyDialog onClose={() => (showImport = false)} />
{/if}

{#if showGenerate}
  <div class="overlay" onclick={() => (showGenerate = false)}>
    <form class="dialog" onclick={(e) => e.stopPropagation()} onsubmit={generate}>
      <h3>Neuen Key generieren</h3>
      <label>
        Name
        <input bind:value={generateName} placeholder="z.B. personal_mac" required autofocus />
      </label>
      <p class="hint">Algorithmus: Ed25519. Datei: <code>~/.ssh/terminalox_&lt;name&gt;</code></p>
      <div class="buttons">
        <button type="button" onclick={() => (showGenerate = false)}>Abbrechen</button>
        <button type="submit" class="primary" disabled={generating}>
          {generating ? "…" : "Generieren"}
        </button>
      </div>
    </form>
  </div>
{/if}

<style>
  .keys-view { display: flex; flex: 1; min-width: 0; }
  .list {
    width: 300px;
    min-width: 300px;
    background: #1a1d23;
    border-right: 1px solid #2a2e37;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    border-bottom: 1px solid #2a2e37;
  }
  header h2 { margin: 0; font-size: 15px; font-weight: 600; color: #e5e7eb; }
  .actions { display: flex; gap: 6px; }
  .actions button {
    background: transparent;
    border: 1px solid #3a3f4b;
    color: #d1d5db;
    padding: 4px 10px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
  }
  .actions button:hover { background: #2a2e37; }

  ul { list-style: none; padding: 6px; margin: 0; overflow-y: auto; flex: 1; }
  .item {
    padding: 10px;
    border-radius: 6px;
    cursor: pointer;
    margin-bottom: 2px;
    color: #d1d5db;
  }
  .item:hover { background: #232732; }
  .item.active { background: #2d3440; }
  .row-top { display: flex; align-items: center; gap: 8px; }
  .algo {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    background: #1e2a3d;
    color: #93c5fd;
    padding: 2px 6px;
    border-radius: 3px;
    letter-spacing: 0.04em;
  }
  .name { font-weight: 500; }
  .fp { font-family: monospace; font-size: 10px; color: #6b7280; margin-top: 3px; word-break: break-all; }

  .tag {
    font-size: 9px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 3px;
    letter-spacing: 0.04em;
  }
  .tag.imported { background: #2d2612; color: #fbbf24; }
  .tag.hardened { background: #052e26; color: #34d399; margin-left: auto; }

  .empty { padding: 30px 20px; color: #6b7280; font-size: 13px; text-align: center; }
  .empty p { margin: 0 0 8px; }
  .empty .small { font-size: 12px; line-height: 1.4; }

  .detail { flex: 1; padding: 20px 24px; overflow-y: auto; background: #0d1017; min-width: 0; }
  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 18px;
    gap: 16px;
  }
  .detail-header h1 { margin: 0; font-size: 22px; color: #e5e7eb; font-weight: 500; }
  .algo-line { margin: 4px 0 0; color: #9ca3af; font-size: 13px; }
  .copy-big {
    padding: 10px 16px;
    border-radius: 8px;
    border: none;
    background: #2563eb;
    color: white;
    cursor: pointer;
    font-size: 14px;
    font-weight: 500;
    white-space: nowrap;
  }
  .copy-big:hover { background: #1d4ed8; }

  .grid { display: flex; flex-direction: column; gap: 12px; }
  .card {
    background: #151923;
    border: 1px solid #2a2e37;
    border-radius: 8px;
    padding: 14px 16px;
    color: #d1d5db;
  }
  .card.danger { border-color: #4c1d1d; }
  .card-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px; }
  .card h3 { margin: 0 0 8px; font-size: 13px; color: #9ca3af; text-transform: uppercase; letter-spacing: 0.04em; font-weight: 600; }
  .card.danger h3 { color: #fca5a5; }

  .ghost-mini {
    background: transparent;
    border: 1px solid #2a2e37;
    color: #9ca3af;
    padding: 3px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 11px;
  }
  .ghost-mini:hover { background: #232732; color: #e5e7eb; }

  .block {
    display: block;
    font-family: 'JetBrains Mono', Menlo, monospace;
    font-size: 12px;
    background: #0a0d14;
    padding: 10px 12px;
    border-radius: 4px;
    color: #93c5fd;
    word-break: break-all;
    white-space: pre-wrap;
    border: 1px solid #1f232b;
  }
  textarea.block {
    width: 100%;
    resize: vertical;
    box-sizing: border-box;
  }
  .muted { color: #6b7280; font-size: 12px; margin: 8px 0 0; line-height: 1.4; }
  code { font-family: 'JetBrains Mono', monospace; font-size: 11px; background: #0a0d14; padding: 1px 5px; border-radius: 3px; color: #93c5fd; }

  .path-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
    font-size: 12px;
  }
  .path-row .label { color: #6b7280; min-width: 60px; }
  .path-row code { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .usage-list { list-style: none; padding: 0; margin: 0; }
  .usage-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 4px;
    font-size: 13px;
  }
  .usage-list li:hover { background: #1a1d24; }
  .status-dot { width: 8px; height: 8px; border-radius: 50%; background: #6b7280; }
  .status-dot.hardened { background: #34d399; }

  .danger-btn {
    margin-top: 10px;
    background: #7f1d1d;
    border: 1px solid #7f1d1d;
    color: white;
    padding: 7px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  .danger-btn:hover:not(:disabled) { background: #991b1b; }
  .danger-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .splash {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #9ca3af;
    text-align: center;
    padding: 40px;
  }
  .splash h1 { color: #e5e7eb; margin: 0 0 16px; font-size: 28px; font-weight: 500; }
  .splash p { max-width: 480px; line-height: 1.6; margin: 4px 0; }
  .splash .small { font-size: 13px; color: #6b7280; margin-top: 14px; }

  /* Inline generate dialog */
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex; align-items: center; justify-content: center;
    z-index: 50;
  }
  .dialog {
    background: #1f232b;
    color: #e5e7eb;
    border-radius: 10px;
    padding: 20px;
    width: 400px;
    border: 1px solid #2a2e37;
  }
  .dialog h3 { margin: 0 0 14px; font-size: 16px; }
  .dialog label { display: block; font-size: 12px; color: #9ca3af; }
  .dialog input {
    width: 100%;
    padding: 8px 10px;
    background: #11141a;
    border: 1px solid #2a2e37;
    color: #e5e7eb;
    border-radius: 6px;
    margin-top: 4px;
    box-sizing: border-box;
    font-size: 14px;
  }
  .hint { font-size: 12px; color: #9ca3af; margin: 8px 0 0; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
  .buttons button {
    padding: 8px 14px;
    border-radius: 6px;
    border: 1px solid #2a2e37;
    background: #2a2e37;
    color: #e5e7eb;
    cursor: pointer;
    font-size: 13px;
  }
  .buttons button.primary { background: #2563eb; border-color: #2563eb; }
  .buttons button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
