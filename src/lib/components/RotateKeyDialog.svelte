<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { RotateReport, Server } from "$lib/types";

  let { server, onClose }: { server: Server; onClose: () => void } = $props();

  const oldKey = $derived(
    server.auth.mode === "key"
      ? app.keys.find((k) => k.id === (server.auth as { key_id: string }).key_id) ?? null
      : null,
  );

  let running = $state(false);
  let error = $state<string | null>(null);
  let report = $state<RotateReport | null>(null);

  async function run() {
    running = true;
    error = null;
    try {
      report = await api.rotateKey(server.id);
      await app.refresh();
      if (report.warnings.length === 0) {
        toast.success(`Key für „${server.name}“ rotiert`);
      }
    } catch (e) {
      error = String(e);
    } finally {
      running = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !running && onClose()} />

<div class="overlay">
  <div class="dialog" role="dialog" aria-modal="true">
    <h3>Key rotieren: {server.name}</h3>

    {#if !report}
      <p>terminalox ersetzt den Key dieses Servers, ohne dass du den Zugang verlierst:</p>
      <ol>
        <li>Neuen Ed25519-Key erzeugen</li>
        <li>Über den aktuellen Key in <code>authorized_keys</code> eintragen</li>
        <li>Mit einer neuen Verbindung prüfen, dass der neue Key funktioniert</li>
        <li>Server-Eintrag umstellen, danach den alten Key vom Server entfernen</li>
      </ol>
      <p class="muted">
        Schlägt Schritt 3 fehl, wird der neue Key wieder entfernt und alles bleibt beim Alten.
        {#if oldKey}Der alte Key „{oldKey.name}“ wird lokal nur gelöscht, wenn kein anderer Server ihn nutzt.{/if}
        Die Härtung bleibt unverändert.
      </p>
    {:else}
      <ul class="result">
        <li class="ok">✓ Neuer Key „{report.newKey.name}“ aktiv</li>
        <li class:ok={report.oldKeyRevoked} class:bad={!report.oldKeyRevoked}>
          {report.oldKeyRevoked ? "✓ Alter Key vom Server entfernt" : "✗ Alter Key noch auf dem Server"}
        </li>
        {#if report.oldKeyRemovedLocally}
          <li class="ok">✓ Alte Key-Dateien lokal gelöscht</li>
        {:else if report.oldKeyStillUsedBy.length > 0}
          <li>Alter Key bleibt lokal — noch genutzt von: {report.oldKeyStillUsedBy.join(", ")}</li>
        {/if}
      </ul>
      {#each report.warnings as w}<p class="warn">⚠ {w}</p>{/each}
    {/if}

    {#if error}<p class="error">{error}</p>{/if}

    <div class="buttons">
      {#if report}
        <button class="primary" onclick={onClose}>Fertig</button>
      {:else}
        <button onclick={onClose} disabled={running}>Abbrechen</button>
        <button class="primary" onclick={run} disabled={running}>
          {running ? "Rotiere …" : "Key rotieren"}
        </button>
      {/if}
    </div>
  </div>
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
    width: 460px;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
    border: 1px solid #2a2e37;
  }
  h3 { margin: 0 0 12px; font-size: 16px; }
  p, li { font-size: 13px; line-height: 1.5; color: #d1d5db; }
  ol { padding-left: 20px; margin: 0 0 10px; }
  code { background: #11141a; padding: 1px 4px; border-radius: 3px; font-size: 11px; }
  .muted { color: #9ca3af; font-size: 12px; }
  .result { list-style: none; padding: 0; }
  .ok { color: #34d399; }
  .bad, .error { color: #f87171; }
  .warn { color: #fbbf24; }
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
