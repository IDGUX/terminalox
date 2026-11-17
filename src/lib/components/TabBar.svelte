<script lang="ts">
  import { app, type View } from "$lib/stores/app.svelte";
  import { i18n, t, type Locale } from "$lib/i18n/index.svelte";

  const tabs = $derived<{ id: View; label: string; icon: string }[]>([
    { id: "servers", label: t("tabs.servers"), icon: "🖥" },
    { id: "keys", label: t("tabs.keys"), icon: "🔑" },
  ]);
  const locales: { id: Locale; label: string }[] = [
    { id: "en", label: "EN" },
    { id: "de", label: "DE" },
  ];
</script>

<nav class="tabs">
  {#each tabs as t (t.id)}
    <button
      class="tab"
      class:active={app.view === t.id}
      onclick={() => (app.view = t.id)}
    >
      <span class="icon">{t.icon}</span>
      <span class="label">{t.label}</span>
    </button>
  {/each}
  <div class="lang" role="radiogroup" aria-label={t("tabs.language")}>
    {#each locales as l (l.id)}
      <button
        class="lang-btn"
        class:active={i18n.locale === l.id}
        role="radio"
        aria-checked={i18n.locale === l.id}
        title={l.id === "en" ? "English (US)" : "Deutsch"}
        onclick={() => i18n.setLocale(l.id)}>{l.label}</button
      >
    {/each}
  </div>
  <div class="brand">terminalox</div>
</nav>

<style>
  .tabs {
    display: flex;
    align-items: center;
    background: #0f1218;
    border-bottom: 1px solid #1f232b;
    padding: 0 8px;
    gap: 2px;
    height: 36px;
  }
  .tab {
    background: transparent;
    border: none;
    color: #6b7280;
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
  }
  .tab:hover { background: #1a1d24; color: #d1d5db; }
  .tab.active { background: #1f232b; color: #e5e7eb; }
  .icon { font-size: 12px; opacity: 0.9; }
  .label { font-weight: 500; }
  .lang {
    margin-left: auto;
    display: flex;
    gap: 2px;
    background: #151923;
    border: 1px solid #1f232b;
    border-radius: 6px;
    padding: 2px;
  }
  .lang-btn {
    background: transparent;
    border: none;
    color: #6b7280;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 4px;
    cursor: pointer;
  }
  .lang-btn:hover { color: #d1d5db; }
  .lang-btn.active { background: #2a2e37; color: #e5e7eb; }
  .brand {
    font-size: 11px;
    color: #4b5563;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 0 8px;
  }
</style>
