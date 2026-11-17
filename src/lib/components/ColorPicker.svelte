<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  let { value = $bindable() }: { value: string | null } = $props();

  const COLORS = $derived<{ hex: string; label: string }[]>([
    { hex: "#ef4444", label: t("color.red") },
    { hex: "#f97316", label: t("color.orange") },
    { hex: "#eab308", label: t("color.yellow") },
    { hex: "#22c55e", label: t("color.green") },
    { hex: "#3b82f6", label: t("color.blue") },
    { hex: "#a855f7", label: t("color.purple") },
  ]);
</script>

<div class="picker" role="radiogroup" aria-label={t("color.label")}>
  <button
    type="button"
    class="swatch none"
    class:sel={!value}
    role="radio"
    aria-checked={!value}
    title={t("color.none")}
    onclick={() => (value = null)}>∅</button>
  {#each COLORS as c (c.hex)}
    <button
      type="button"
      class="swatch"
      class:sel={value === c.hex}
      role="radio"
      aria-checked={value === c.hex}
      title={c.label}
      style:background={c.hex}
      onclick={() => (value = c.hex)}
    ></button>
  {/each}
</div>

<style>
  .picker { display: flex; gap: 8px; margin-top: 6px; }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid transparent;
    padding: 0;
    cursor: pointer;
    outline: none;
  }
  .swatch.none {
    background: #11141a;
    border-color: #2a2e37;
    color: #6b7280;
    font-size: 12px;
    line-height: 1;
  }
  .swatch.sel { box-shadow: 0 0 0 2px #1f232b, 0 0 0 4px #e5e7eb; }
  .swatch:focus-visible { box-shadow: 0 0 0 2px #1f232b, 0 0 0 4px #3b82f6; }
</style>
