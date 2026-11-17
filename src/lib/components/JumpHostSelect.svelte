<script lang="ts">
  import { app } from "$lib/stores/app.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import type { Uuid } from "$lib/types";

  let {
    value = $bindable(""),
    excludeId = null,
  }: {
    value?: Uuid | "";
    /** The server being edited — cannot be its own jump host. */
    excludeId?: Uuid | null;
  } = $props();

  /** Servers whose jump chain passes through `excludeId` would create a loop. */
  function leadsTo(id: Uuid, target: Uuid | null): boolean {
    if (!target) return false;
    const seen = new Set<Uuid>();
    let cur: Uuid | null | undefined = id;
    while (cur && !seen.has(cur)) {
      if (cur === target) return true;
      seen.add(cur);
      cur = app.servers.find((s) => s.id === cur)?.jump_server_id;
    }
    return false;
  }

  const candidates = $derived(
    app.servers
      .filter((s) => !leadsTo(s.id, excludeId))
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" })),
  );
</script>

{#if candidates.length > 0}
  <label>
    {t("jump.label")}
    <select bind:value>
      <option value="">— {t("jump.direct")} —</option>
      {#each candidates as s (s.id)}
        <option value={s.id}>{s.name} ({s.user}@{s.host})</option>
      {/each}
    </select>
  </label>
  {#if value}
    <p class="hint">{t("jump.hint")}</p>
  {/if}
{/if}

<style>
  label { display: block; font-size: 12px; color: #9ca3af; margin-bottom: 10px; }
  select {
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
  select:focus { outline: none; border-color: #3b82f6; }
  .hint { font-size: 12px; color: #9ca3af; margin: -4px 0 10px; line-height: 1.5; }
  .hint code { background: #11141a; padding: 1px 4px; border-radius: 3px; font-size: 11px; }
</style>
