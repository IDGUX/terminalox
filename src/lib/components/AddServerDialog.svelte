<script lang="ts">
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";

  let { onClose }: { onClose: () => void } = $props();

  let name = $state("");
  let host = $state("");
  let port = $state(22);
  let user = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);

  async function submit(e: Event) {
    e.preventDefault();
    error = null;
    submitting = true;
    try {
      const s = await app.addServer({
        name: name.trim(),
        host: host.trim(),
        port,
        user: user.trim(),
        auth: { mode: "password" },
        color: null,
      });
      toast.success(`Server „${s.name}" hinzugefügt`);
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
    <h3>Neuer Server</h3>
    <label>
      Name
      <input bind:value={name} placeholder="z.B. prod-web" required autofocus />
    </label>
    <div class="row">
      <label class="grow">
        Host
        <input bind:value={host} placeholder="example.com" required />
      </label>
      <label>
        Port
        <input type="number" bind:value={port} min="1" max="65535" />
      </label>
    </div>
    <label>
      User
      <input bind:value={user} placeholder="root" required />
    </label>

    <p class="hint">
      Auth: Passwort (für die erste Verbindung). Danach kannst du mit einem Klick
      einen SSH-Key deployen und die App schaltet auf Key-Auth um.
    </p>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="buttons">
      <button type="button" onclick={onClose}>Abbrechen</button>
      <button type="submit" class="primary" disabled={submitting}>Hinzufügen</button>
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
  input:focus { outline: none; border-color: #3b82f6; }
  .row { display: flex; gap: 10px; }
  .row .grow { flex: 1; }
  .row label:last-child { width: 90px; }
  .hint { font-size: 12px; color: #9ca3af; margin: 10px 0; }
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
