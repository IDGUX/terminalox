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
  import SecretDialog from "$lib/components/SecretDialog.svelte";
  import RotateKeyDialog from "$lib/components/RotateKeyDialog.svelte";
  import ConfigImportDialog from "$lib/components/ConfigImportDialog.svelte";
  import { app } from "$lib/stores/app.svelte";
  import { errorText, i18n, t } from "$lib/i18n/index.svelte";
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
  let rotateFor = $state<Server | null>(null);
  let configImport = $state(false);
  let loadError = $state<string | null>(null);

  $effect(() => {
    document.documentElement.lang = i18n.locale;
  });

  onMount(async () => {
    // Dev-only demo data for screenshots (`pnpm dev`, then open /?demo).
    // The DEV guard lets production builds drop the demo module.
    const params = new URLSearchParams(location.search);
    const demo =
      import.meta.env.DEV && params.has("demo")
        ? (await import("$lib/demo")).setupDemo(params)
        : null;
    try {
      await app.refresh();
    } catch (e) {
      loadError = errorText(e);
    }
    if (demo) {
      app.view = demo.view;
      app.selectedKeyId = demo.keyId;
      const s = app.servers.find((x) => x.id === demo.serverId) ?? null;
      app.selectedServerId = s?.id ?? null;
      if (s && demo.dialog === "deploy") deployFor = s;
      if (s && demo.dialog === "harden")
        hardenFor = { server: s, readiness: { status: "ready", method: "sudo_with_password" } };
      if (s && demo.dialog === "rotate") rotateFor = s;
      if (demo.dialog === "import") configImport = true;
    }
  });
</script>

<div class="app">
  <TabBar />

  <div class="body">
    {#if loadError}
      <div class="splash error">{t("page.loadError", { error: loadError })}</div>
    {:else if app.view === "servers"}
      <Sidebar
        onAdd={(folderId) => (addDialog = { folderId: folderId ?? null, template: null })}
        onDuplicate={(s) => (addDialog = { folderId: null, template: s })}
        onEdit={(s) => (editFor = s)}
        onDeploy={(s) => (deployFor = s)}
        onHarden={(s, r) => (hardenFor = { server: s, readiness: r })}
        onDelete={(s) => (deleteFor = s)}
        onRotate={(s) => (rotateFor = s)}
        onExport={() => (backupMode = "export")}
        onImport={() => (backupMode = "import")}
        onImportConfig={() => (configImport = true)}
      />
      <main class="main">
        {#if app.selectedServer}
          {#key app.selectedServer.id}
            <TerminalSplitView server={app.selectedServer} />
          {/key}
        {:else}
          <div class="splash">
            <h1>{t("page.title")}</h1>
            <p>{t("page.intro")}</p>
            <p class="small">{t("page.workflow")}</p>
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

  {#if rotateFor}
    <RotateKeyDialog server={rotateFor} onClose={() => (rotateFor = null)} />
  {/if}
  {#if configImport}
    <ConfigImportDialog onClose={() => (configImport = false)} />
  {/if}

  <HostKeyDialog />
  <SecretDialog />
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
