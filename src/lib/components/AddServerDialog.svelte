<script lang="ts">
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { AuthMode, Server, Uuid } from "$lib/types";
  import ColorPicker from "./ColorPicker.svelte";

  let {
    onClose,
    folderId: initialFolderId = null,
    template = null,
  }: {
    onClose: () => void;
    folderId?: Uuid | null;
    /** Vorlage beim Duplizieren — alles außer Härtungs-Status wird übernommen. */
    template?: Server | null;
  } = $props();

  let name = $state(template ? `${template.name} (Kopie)` : "");
  let host = $state(template?.host ?? "");
  let port = $state(template?.port ?? 22);
  let user = $state(template?.user ?? "");
  let authMode = $state<"password" | "key">(template?.auth.mode ?? "password");
  let keyId = $state<Uuid | "">(template?.auth.mode === "key" ? template.auth.key_id : "");
  let folderId = $state<Uuid | "">(template?.folder_id ?? initialFolderId ?? "");
  let color = $state<string | null>(template?.color ?? null);
  let submitting = $state(false);
  let error = $state<string | null>(null);

  const keys = $derived(app.keys);

  // Auto-pick first key when switching to key-mode if nothing selected yet.
  $effect(() => {
    if (authMode === "key" && !keyId && keys.length > 0) {
      keyId = keys[0].id;
    }
  });

  async function submit(e: Event) {
    e.preventDefault();
    error = null;
    let auth: AuthMode;
    if (authMode === "key") {
      if (!keyId) {
        error = "Bitte einen Key auswählen.";
        return;
      }
      auth = { mode: "key", key_id: keyId };
    } else {
      auth = { mode: "password" };
    }
    submitting = true;
    try {
      const s = await app.addServer({
        name: name.trim(),
        host: host.trim(),
        port,
        user: user.trim(),
        auth,
        color,
        folder_id: folderId || null,
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

<svelte:window
  onkeydown={(e) => e.key === "Escape" && !(submitting) && onClose()}
/>

<!-- Kein Schließen per Klick daneben: ein verirrter Klick würde alle Eingaben verwerfen. -->
<div class="overlay">
  <form class="dialog" onsubmit={submit}>
    <h3>{template ? "Server duplizieren" : "Neuer Server"}</h3>
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

    {#if authMode === "password"}
      <p class="hint">
        Für die erste Verbindung mit Passwort. Danach kannst du mit einem Klick
        einen SSH-Key deployen und die App schaltet auf Key-Auth um.
      </p>
    {:else if keys.length === 0}
      <p class="hint warn">
        Du hast noch keinen Key in Terminalox. Wechsle in den <strong>Keys</strong>-Tab,
        erstelle oder importiere einen Key, und kopiere den Public Key in die
        Server-Bestellung (z.B. Hetzner) bevor du hier auf <em>Hinzufügen</em> klickst.
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
        Der Public Key dieses Keys muss schon auf dem Server in
        <code>~/.ssh/authorized_keys</code> liegen (z.B. von Hetzner beim
        Bestellen injiziert).
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
        Hinzufügen
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
