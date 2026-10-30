<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";

  let { onClose }: { onClose: () => void } = $props();

  let sourcePath = $state("");
  let name = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);

  async function pickFile() {
    const home = await (async () => {
      try {
        const { homeDir } = await import("@tauri-apps/api/path");
        return await homeDir();
      } catch {
        return undefined;
      }
    })();
    const selected = await openDialog({
      multiple: false,
      directory: false,
      defaultPath: home ? `${home}/.ssh` : undefined,
      title: "Private Key wählen",
    });
    if (typeof selected === "string") {
      sourcePath = selected;
      if (!name.trim()) {
        const base = selected.split("/").pop() ?? "imported";
        name = base.replace(/\.pub$/, "").replace(/[^a-zA-Z0-9_-]/g, "_");
      }
    }
  }

  async function submit(e: Event) {
    e.preventDefault();
    error = null;
    submitting = true;
    try {
      const key = await app.importKey(sourcePath, name.trim());
      toast.success(`Key „${key.name}" importiert (${key.algorithm})`);
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
    <h3>SSH-Key importieren</h3>
    <p class="hint">
      Wähle einen bestehenden Private Key (z.B. <code>~/.ssh/id_ed25519</code>). Die Datei
      wird nach <code>~/.ssh/terminalox_imported_&lt;name&gt;</code> kopiert und im Store
      registriert. <strong>Das Original bleibt unangetastet.</strong> Verschlüsselte Keys
      (mit Passphrase) werden noch nicht unterstützt.
    </p>

    <label>
      Pfad zum Private Key
      <div class="row">
        <input bind:value={sourcePath} placeholder="/Users/you/.ssh/id_ed25519" required />
        <button type="button" onclick={pickFile}>Durchsuchen…</button>
      </div>
    </label>

    <label>
      Name in terminalox
      <input bind:value={name} placeholder="z.B. personal_laptop" required />
    </label>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="buttons">
      <button type="button" onclick={onClose}>Abbrechen</button>
      <button type="submit" class="primary" disabled={submitting}>
        {submitting ? "…" : "Importieren"}
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
    width: 480px;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
    border: 1px solid #2a2e37;
  }
  h3 { margin: 0 0 10px; font-size: 16px; }
  .hint { font-size: 12px; color: #9ca3af; margin: 0 0 14px; line-height: 1.5; }
  label { display: block; font-size: 12px; color: #9ca3af; margin-bottom: 10px; }
  .row { display: flex; gap: 8px; align-items: stretch; margin-top: 4px; }
  .row input { flex: 1; }
  .row button {
    background: #2a2e37; border: 1px solid #2a2e37; color: #e5e7eb;
    padding: 0 12px; border-radius: 6px; cursor: pointer; font-size: 13px;
  }
  input {
    padding: 8px 10px;
    background: #11141a;
    border: 1px solid #2a2e37;
    color: #e5e7eb;
    border-radius: 6px;
    box-sizing: border-box;
    font-size: 14px;
    width: 100%;
  }
  code { background: #11141a; padding: 1px 5px; border-radius: 3px; font-size: 12px; }
  .error { color: #f87171; font-size: 13px; margin: 4px 0 0; }
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
