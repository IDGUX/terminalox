<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { AuthMode, Server, Uuid } from "$lib/types";
  import ColorPicker from "./ColorPicker.svelte";

  let { server, onClose }: { server: Server; onClose: () => void } = $props();

  let name = $state(server.name);
  let host = $state(server.host);
  let port = $state(server.port);
  let user = $state(server.user);
  let authMode = $state<"password" | "key">(server.auth.mode);
  let keyId = $state<Uuid | "">(
    server.auth.mode === "key" ? server.auth.key_id : "",
  );
  let folderId = $state<Uuid | "">(server.folder_id ?? "");
  let color = $state<string | null>(server.color ?? null);
  let submitting = $state(false);
  let error = $state<string | null>(null);

  const originalId = server.id;
  const originalAuth = server.auth;
  const keys = $derived(app.keys);

  $effect(() => {
    if (authMode === "key" && !keyId && keys.length > 0) {
      keyId = keys[0].id;
    }
  });

  function authChanged(): AuthMode | null {
    if (authMode === "password") {
      if (originalAuth.mode === "password") return null;
      return { mode: "password" };
    }
    if (!keyId) return null;
    if (originalAuth.mode === "key" && originalAuth.key_id === keyId)
      return null;
    return { mode: "key", key_id: keyId };
  }

  async function submit(e: Event) {
    e.preventDefault();
    error = null;
    if (authMode === "key" && !keyId) {
      error = "Bitte einen Key auswählen.";
      return;
    }
    submitting = true;
    try {
      const auth = authChanged();
      const updated = await api.updateServer({
        id: originalId,
        name: name.trim(),
        host: host.trim(),
        port,
        user: user.trim(),
        color,
        ...(auth ? { auth } : {}),
      });
      app.servers = app.servers.map((s) => (s.id === updated.id ? updated : s));
      if ((folderId || null) !== (server.folder_id ?? null)) {
        await app.moveServer(originalId, folderId || null);
      }
      toast.success(`Server „${updated.name}" aktualisiert`);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && !(submitting) && onClose()}
/>

<!-- Kein Schließen per Klick daneben: ein verirrter Klick würde alle Eingaben verwerfen. -->
<div class="overlay">
  <form class="dialog" onsubmit={submit}>
    <h3>Server bearbeiten</h3>

    <label>
      Name
      <input bind:value={name} required autofocus />
    </label>
    <div class="row">
      <label class="grow">
        Host
        <input bind:value={host} required />
      </label>
      <label>
        Port
        <input type="number" bind:value={port} min="1" max="65535" />
      </label>
    </div>
    <label>
      User
      <input bind:value={user} required />
    </label>

    {#if app.folders.length > 0}
      <label>
        Ordner
        <select bind:value={folderId}>
          <option value="">— Ohne Ordner —</option>
          {#each app.folders as f (f.id)}
            <option value={f.id}>{f.name}</option>
          {/each}
        </select>
      </label>
    {/if}

    <div class="color-field">
      Farbe
      <ColorPicker bind:value={color} />
    </div>

    <fieldset class="auth">
      <legend>Auth</legend>
      <label class="radio">
        <input type="radio" name="auth" value="password" bind:group={authMode} />
        <span>Passwort</span>
      </label>
      <label class="radio">
        <input type="radio" name="auth" value="key" bind:group={authMode} />
        <span>Bestehender Key</span>
      </label>
    </fieldset>

    {#if authMode === "key"}
      {#if keys.length === 0}
        <p class="hint warn">
          Keine Keys in Terminalox vorhanden. Lege erst einen unter
          <strong>Keys</strong> an.
        </p>
      {:else}
        <label>
          Key
          <select bind:value={keyId} required>
            {#each keys as k}
              <option value={k.id}>{k.name} ({k.algorithm})</option>
            {/each}
          </select>
        </label>
        <p class="hint">
          Der Public Key muss auf dem Server in
          <code>~/.ssh/authorized_keys</code> liegen, sonst schlägt die
          Verbindung fehl.
        </p>
      {/if}
    {:else if server.hardened}
      <p class="hint warn">
        Dieser Server ist gehärtet — Passwort-Login ist serverseitig deaktiviert.
        Auf Passwort umstellen führt zu Verbindungsfehlern, bis der Server
        ent-härtet wird.
      </p>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}

    <div class="buttons">
      <button type="button" onclick={onClose}>Abbrechen</button>
      <button
        type="submit"
        class="primary"
        disabled={submitting || (authMode === "key" && keys.length === 0)}
      >
        {submitting ? "…" : "Speichern"}
      </button>
    </div>
  </form>
</div>

<style>
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
    width: 420px;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
    border: 1px solid #2a2e37;
  }
  h3 { margin: 0 0 14px; font-size: 16px; }
  label { display: block; font-size: 12px; color: #9ca3af; margin-bottom: 10px; }
  input, select {
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
  input:focus, select:focus { outline: none; border-color: #3b82f6; }
  .row { display: flex; gap: 10px; }
  .row .grow { flex: 1; }
  .row label:last-child { width: 90px; }
  .auth {
    border: 1px solid #2a2e37;
    border-radius: 6px;
    padding: 8px 12px 4px;
    margin: 4px 0 10px;
    display: flex;
    gap: 16px;
  }
  .auth legend { font-size: 12px; color: #9ca3af; padding: 0 4px; }
  .radio {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: #e5e7eb;
    margin: 0;
    cursor: pointer;
  }
  .radio input { width: auto; margin: 0; }
  .hint { font-size: 12px; color: #9ca3af; margin: 4px 0 10px; line-height: 1.5; }
  .hint.warn { color: #fbbf24; }
  .hint code { background: #11141a; padding: 1px 4px; border-radius: 3px; font-size: 11px; }
  .color-field { font-size: 12px; color: #9ca3af; margin-bottom: 12px; }
  .error { color: #f87171; font-size: 13px; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
  button {
    padding: 8px 14px;
    border-radius: 6px;
    border: 1px solid #2a2e37;
    background: #2a2e37;
    color: #e5e7eb;
    cursor: pointer;
    font-size: 13px;
  }
  button.primary { background: #2563eb; border-color: #2563eb; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
