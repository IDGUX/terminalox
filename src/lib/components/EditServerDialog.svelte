<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { Server } from "$lib/types";

  let { server, onClose }: { server: Server; onClose: () => void } = $props();

  let name = $state(server.name);
  let host = $state(server.host);
  let port = $state(server.port);
  let user = $state(server.user);
  let submitting = $state(false);
  let error = $state<string | null>(null);

  const originalId = server.id;

  async function submit(e: Event) {
    e.preventDefault();
    error = null;
    submitting = true;
    try {
      const updated = await api.updateServer({
        id: originalId,
        name: name.trim(),
        host: host.trim(),
        port,
        user: user.trim(),
      });
      app.servers = app.servers.map((s) => (s.id === updated.id ? updated : s));
      toast.success(`Server „${updated.name}" aktualisiert`);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<div class="overlay" onclick={onClose}>
  <form class="dialog" onclick={(e) => e.stopPropagation()} onsubmit={submit}>
    <h3>Server bearbeiten</h3>
    <p class="hint">
      Auth-Modus (Passwort vs. Key) kannst du hier nicht ändern — der wird durch den
      Deploy-Flow gesetzt. Um ihn umzustellen, Server löschen + neu anlegen oder neu
      deployen.
    </p>

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

    {#if error}<p class="error">{error}</p>{/if}

    <div class="buttons">
      <button type="button" onclick={onClose}>Abbrechen</button>
      <button type="submit" class="primary" disabled={submitting}>
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
  h3 { margin: 0 0 8px; font-size: 16px; }
  .hint { font-size: 12px; color: #9ca3af; margin: 0 0 14px; line-height: 1.5; }
  label { display: block; font-size: 12px; color: #9ca3af; margin-bottom: 10px; }
  input {
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
  .row { display: flex; gap: 10px; }
  .row .grow { flex: 1; }
  .row label:last-child { width: 90px; }
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
