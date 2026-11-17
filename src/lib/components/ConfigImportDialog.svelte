<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { t, tm, tn } from "$lib/i18n/index.svelte";
  import type { ConfigImportReport, ImportCandidate } from "$lib/types";

  let { onClose }: { onClose: () => void } = $props();

  let candidates = $state<ImportCandidate[] | null>(null);
  let selected = $state<Record<string, boolean>>({});
  let loading = $state(true);
  let importing = $state(false);
  let error = $state<string | null>(null);
  let report = $state<ConfigImportReport | null>(null);

  const importable = (c: ImportCandidate) => !c.problem && !c.alreadyExists;
  const count = $derived(Object.values(selected).filter(Boolean).length);

  onMount(async () => {
    try {
      const list = await api.previewSshConfigImport();
      candidates = list;
      selected = Object.fromEntries(list.map((c) => [c.alias, importable(c)]));
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  async function runImport() {
    importing = true;
    error = null;
    try {
      const aliases = Object.entries(selected)
        .filter(([, v]) => v)
        .map(([k]) => k);
      report = await api.importSshConfigHosts(aliases);
      await app.refresh();
      toast.success(tn("cfgimport.toast", report.serversAdded));
    } catch (e) {
      error = String(e);
    } finally {
      importing = false;
    }
  }

  function authLabel(c: ImportCandidate): string {
    if (c.auth === "key") return `🔑 ${t("cfgimport.authKey")}`;
    if (c.auth === "agent") return "ssh-agent";
    return `🔓 ${t("server.authPassword")}`;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !importing && onClose()} />

<div class="overlay">
  <div class="dialog" role="dialog" aria-modal="true">
    <h3>{t("cfgimport.title")}</h3>

    {#if loading}
      <p class="muted">{t("cfgimport.loading")}</p>
    {:else if report}
      <p>
        {tn("cfgimport.added", report.serversAdded)}{report.keysImported > 0
          ? ", " + tn("cfgimport.keys", report.keysImported)
          : ""}.
      </p>
      {#each report.warnings as w}<p class="warn">⚠ {w.alias}: {tm(w.msg)}</p>{/each}
    {:else if candidates && candidates.length === 0}
      <p class="muted">{t("cfgimport.none")}</p>
    {:else if candidates}
      <p class="muted">{t("cfgimport.intro")}</p>
      <ul class="list">
        {#each candidates as c (c.alias)}
          <li class:disabled={!importable(c)}>
            <label>
              <input type="checkbox" bind:checked={selected[c.alias]} disabled={!importable(c)} />
              <span class="main">
                <strong>{c.alias}</strong>
                <span class="muted">{c.effectiveUser}@{c.host}{c.port !== 22 ? `:${c.port}` : ""}</span>
              </span>
              <span class="meta">
                {#if c.problem}<span class="bad">{tm(c.problem)}</span>
                {:else if c.alreadyExists}<span class="muted">{t("cfgimport.exists")}</span>
                {:else}
                  {authLabel(c)}{#if c.proxyJump}{" · "}{t("cfgimport.via", { jump: c.proxyJump })}{/if}
                {/if}
              </span>
            </label>
          </li>
        {/each}
      </ul>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}

    <div class="buttons">
      {#if report}
        <button class="primary" onclick={onClose}>{t("common.done")}</button>
      {:else}
        <button onclick={onClose} disabled={importing}>{t("common.cancel")}</button>
        <button class="primary" onclick={runImport} disabled={importing || count === 0}>
          {importing ? t("cfgimport.running") : t("cfgimport.submit", { count })}
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
    width: 560px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
    border: 1px solid #2a2e37;
  }
  h3 { margin: 0 0 12px; font-size: 16px; }
  p { font-size: 13px; line-height: 1.5; color: #d1d5db; margin: 0 0 10px; }
  code { background: #11141a; padding: 1px 4px; border-radius: 3px; font-size: 11px; }
  .muted { color: #9ca3af; font-size: 12px; }
  .list { list-style: none; padding: 0; margin: 0; overflow-y: auto; border: 1px solid #2a2e37; border-radius: 6px; }
  .list li { border-bottom: 1px solid #2a2e37; }
  .list li:last-child { border-bottom: none; }
  .list li.disabled { opacity: 0.55; }
  .list label { display: flex; align-items: center; gap: 10px; padding: 8px 10px; cursor: pointer; font-size: 13px; }
  .main { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .main .muted { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: 12px; color: #9ca3af; text-align: right; }
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
