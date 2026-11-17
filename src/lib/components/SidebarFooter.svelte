<script lang="ts">
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";

  let {
    onExport,
    onImport,
    onImportConfig,
  }: {
    onExport: () => void;
    onImport: () => void;
    onImportConfig: () => void;
  } = $props();
</script>

<footer>
  <button
    class="ghost"
    onclick={async () => {
      try {
        const p = await app.syncSshConfig();
        toast.success(`~/.ssh/config geschrieben (${p})`);
      } catch (e) {
        toast.error(String(e));
      }
    }}
    title="terminalox-Server als verwalteten Block in ~/.ssh/config eintragen"
  >
    ~/.ssh/config schreiben
  </button>
  <button class="ghost" onclick={onImportConfig} title="Hosts aus ~/.ssh/config übernehmen">
    ~/.ssh/config importieren …
  </button>
  <div class="backup-row">
    <button class="ghost-half" onclick={onExport} title="Backup exportieren">↑ Export</button>
    <button class="ghost-half" onclick={onImport} title="Backup importieren">↓ Import</button>
  </div>
</footer>

<style>
  footer { padding: 10px; border-top: 1px solid #2a2e37; }
  button.ghost {
    width: 100%;
    background: transparent;
    color: #9ca3af;
    border: 1px solid #2a2e37;
    padding: 6px;
    border-radius: 6px;
    font-size: 12px;
    cursor: pointer;
  }
  button.ghost:hover { background: #232732; }

  .backup-row { display: flex; gap: 6px; margin-top: 6px; }
  button.ghost-half {
    flex: 1;
    background: transparent;
    color: #9ca3af;
    border: 1px solid #2a2e37;
    padding: 6px;
    border-radius: 6px;
    font-size: 11px;
    cursor: pointer;
  }
  button.ghost-half:hover { background: #232732; color: #e5e7eb; }
  footer button.ghost + button.ghost { margin-top: 6px; }
</style>
