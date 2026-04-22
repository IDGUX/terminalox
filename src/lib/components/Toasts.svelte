<script lang="ts">
  import { toast } from "$lib/stores/toast.svelte";
</script>

<div class="toast-stack" aria-live="polite">
  {#each toast.items as t (t.id)}
    <div class="toast {t.kind}">
      <span class="icon">
        {#if t.kind === "success"}✓{:else if t.kind === "error"}✕{:else}ⓘ{/if}
      </span>
      <span class="msg">{t.message}</span>
      <button class="close" onclick={() => toast.dismiss(t.id)} aria-label="Schließen">×</button>
    </div>
  {/each}
</div>

<style>
  .toast-stack {
    position: fixed;
    bottom: 20px;
    right: 20px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 100;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-radius: 8px;
    background: #1f232b;
    color: #e5e7eb;
    border: 1px solid #2a2e37;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    font-size: 13px;
    min-width: 280px;
    max-width: 420px;
    pointer-events: auto;
    animation: slide-in 0.15s ease-out;
  }
  .toast.success { border-left: 3px solid #34d399; }
  .toast.error { border-left: 3px solid #f87171; }
  .toast.info { border-left: 3px solid #60a5fa; }
  .icon { font-weight: 700; opacity: 0.9; font-size: 14px; }
  .toast.success .icon { color: #34d399; }
  .toast.error .icon { color: #f87171; }
  .toast.info .icon { color: #60a5fa; }
  .msg { flex: 1; line-height: 1.35; }
  .close {
    background: transparent;
    border: none;
    color: #6b7280;
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
    padding: 0 4px;
  }
  .close:hover { color: #e5e7eb; }
  @keyframes slide-in {
    from { transform: translateX(20px); opacity: 0; }
    to   { transform: translateX(0);    opacity: 1; }
  }
</style>
