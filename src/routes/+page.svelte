<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import TabBar from "$lib/components/TabBar.svelte";
  import AddServerDialog from "$lib/components/AddServerDialog.svelte";
  import EditServerDialog from "$lib/components/EditServerDialog.svelte";
  import DeployKeyDialog from "$lib/components/DeployKeyDialog.svelte";
  import DeleteServerDialog from "$lib/components/DeleteServerDialog.svelte";
  import HardenServerDialog from "$lib/components/HardenServerDialog.svelte";
  import TerminalSplitView from "$lib/components/TerminalSplitView.svelte";
  import KeysView from "$lib/components/KeysView.svelte";
  import BackupDialog from "$lib/components/BackupDialog.svelte";
  import Toasts from "$lib/components/Toasts.svelte";
  import HostKeyDialog from "$lib/components/HostKeyDialog.svelte";
  import { app } from "$lib/stores/app.svelte";
  import type { HardenReadiness, Server, Uuid } from "$lib/types";

  let addDialog = $state<{ folderId: Uuid | null; template: Server | null } | null>(null);
  let editFor = $state<Server | null>(null);
  let deployFor = $state<Server | null>(null);
  let hardenFor = $state<
    | { server: Server; readiness: HardenReadiness & { status: "ready" } }
    | null
  >(null);
  let deleteFor = $state<Server | null>(null);
  let backupMode = $state<"export" | "import" | null>(null);
  let loadError = $state<string | null>(null);

  onMount(async () => {
    try {
      await app.refresh();
    } catch (e) {
      loadError = String(e);
    }
  });
</script>

<div class="app">
  <TabBar />

  <div class="body">
    {#if loadError}
      <div class="splash error">Fehler beim Laden: {loadError}</div>
    {:else if app.view === "servers"}
      <Sidebar
        onAdd={(folderId) => (addDialog = { folderId: folderId ?? null, template: null })}
        onDuplicate={(s) => (addDialog = { folderId: null, template: s })}
        onEdit={(s) => (editFor = s)}
        onDeploy={(s) => (deployFor = s)}
        onHarden={(s, r) => (hardenFor = { server: s, readiness: r })}
        onDelete={(s) => (deleteFor = s)}
        onExport={() => (backupMode = "export")}
        onImport={() => (backupMode = "import")}
      />
      <main class="main">
        {#if app.selectedServer}
          {#key app.selectedServer.id}
            <TerminalSplitView server={app.selectedServer} />
          {/key}
        {:else}
          <div class="splash">
            <h1>Servers</h1>
            <p>Wähle einen Server links, oder füge einen neuen hinzu (<strong>+</strong>).</p>
            <p class="small">
              Workflow: Server hinzufügen → einmal mit Passwort verbinden →
              „SSH-Key deployen + Härten" → ab jetzt ohne Passwort,
              Passwort-Login serverseitig aus.
            </p>
          </div>
        {/if}
      </main>
    {:else if app.view === "keys"}
      <KeysView />
    {/if}
  </div>

  {#if addDialog}
    <AddServerDialog
      folderId={addDialog.folderId}
      template={addDialog.template}
      onClose={() => (addDialog = null)}
    />
  {/if}
  {#if editFor}
    <EditServerDialog server={editFor} onClose={() => (editFor = null)} />
  {/if}
  {#if deployFor}
    <DeployKeyDialog server={deployFor} onClose={() => (deployFor = null)} />
  {/if}
  {#if hardenFor}
    <HardenServerDialog
      server={hardenFor.server}
      readiness={hardenFor.readiness}
      onClose={() => (hardenFor = null)}
    />
  {/if}
  {#if deleteFor}
    <DeleteServerDialog server={deleteFor} onClose={() => (deleteFor = null)} />
  {/if}
  {#if backupMode}
    <BackupDialog mode={backupMode} onClose={() => (backupMode = null)} />
  {/if}

  <HostKeyDialog />
  <Toasts />
</div>

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    height: 100%;
    background: #0d1017;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  }
  :global(body) { color-scheme: dark; }
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .splash {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: #9ca3af;
    padding: 40px;
    text-align: center;
  }
  .splash h1 { color: #e5e7eb; margin: 0 0 16px; font-size: 32px; font-weight: 500; }
  .splash p { max-width: 520px; line-height: 1.6; }
  .splash .small { font-size: 13px; color: #6b7280; margin-top: 20px; }
  .splash.error { color: #f87171; }
</style>
