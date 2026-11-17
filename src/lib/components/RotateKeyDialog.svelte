<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { t, tm } from "$lib/i18n/index.svelte";
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
        toast.success(t("rotate.toast", { name: server.name }));
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
    <h3>{t("rotate.title", { name: server.name })}</h3>

    {#if !report}
      <p>{t("rotate.intro")}</p>
      <ol>
        <li>{t("rotate.step1")}</li>
        <li>{t("rotate.step2")}</li>
        <li>{t("rotate.step3")}</li>
        <li>{t("rotate.step4")}</li>
      </ol>
      <p class="muted">
        {t("rotate.safety")}
        {#if oldKey}{t("rotate.oldKeyLocal", { name: oldKey.name })}{/if}
        {t("rotate.hardeningUnchanged")}
      </p>
    {:else}
      <ul class="result">
        <li class="ok">✓ {t("rotate.newActive", { name: report.newKey.name })}</li>
        <li class:ok={report.oldKeyRevoked} class:bad={!report.oldKeyRevoked}>
          {report.oldKeyRevoked ? `✓ ${t("rotate.oldRevoked")}` : `✗ ${t("rotate.oldStill")}`}
        </li>
        {#if report.oldKeyRemovedLocally}
          <li class="ok">✓ {t("rotate.oldDeleted")}</li>
        {:else if report.oldKeyStillUsedBy.length > 0}
          <li>{t("rotate.oldKept", { servers: report.oldKeyStillUsedBy.join(", ") })}</li>
        {/if}
      </ul>
      {#each report.warnings as w}<p class="warn">⚠ {tm(w)}</p>{/each}
    {/if}

    {#if error}<p class="error">{error}</p>{/if}

    <div class="buttons">
      {#if report}
        <button class="primary" onclick={onClose}>{t("common.done")}</button>
      {:else}
        <button onclick={onClose} disabled={running}>{t("common.cancel")}</button>
        <button class="primary" onclick={run} disabled={running}>
          {running ? t("rotate.running") : t("actions.rotate")}
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
