<script lang="ts">
  import { app, type View } from "$lib/stores/app.svelte";

  const tabs: { id: View; label: string; icon: string }[] = [
    { id: "servers", label: "Servers", icon: "🖥" },
    { id: "keys", label: "Keys", icon: "🔑" },
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
  .brand {
    margin-left: auto;
    font-size: 11px;
    color: #4b5563;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 0 8px;
  }
</style>
