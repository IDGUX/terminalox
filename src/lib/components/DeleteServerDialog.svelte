<script lang="ts">
  import { invoke } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { Server } from "$lib/types";

  let { server, onClose }: { server: Server; onClose: () => void } = $props();

  interface DeleteReport {
    serverRemoved: boolean;
    remoteUnhardenError: string | null;
    remoteCleanupError: string | null;
    localKeyError: string | null;
    keyStillReferenced: boolean;
    hardeningKept: boolean;
    hardeningRestored: boolean;
    hardeningRestoreError: string | null;
  }

  let cleanupRemote = $state(true);
  let deleteUnusedLocalKey = $state(false);
  let password = $state("");
  let otherKeyConfirmed = $state(false);
  let sudoPassword = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let remoteError = $state<string | null>(null);

  const hasKeyAuth = $derived(server.auth.mode === "key");
  const usePassword = $derived(password.length > 0);
  // Unharden (and a possible re-harden) only runs on the password path.
  const showSudo = $derived(
    hasKeyAuth && cleanupRemote && usePassword && server.hardened && server.user !== "root",
  );

  async function remove(localOnly: boolean) {
    error = null;
    submitting = true;
    try {
      const cleanUpOnServer = !localOnly && hasKeyAuth && cleanupRemote;
      const removeUnusedKey = !localOnly && hasKeyAuth && !cleanupRemote && deleteUnusedLocalKey;
      const report = await invoke<DeleteReport>("delete_server_full", {
        opts: {
          id: server.id,
          cleanupRemote: cleanUpOnServer,
          deleteUnusedLocalKey: removeUnusedKey,
          password: cleanUpOnServer && usePassword ? password : null,
          otherKeyConfirmed: cleanUpOnServer && !usePassword && otherKeyConfirmed,
          sudoPassword: cleanUpOnServer && showSudo && sudoPassword ? sudoPassword : null,
          force: localOnly,
        },
      });
      if (!report.serverRemoved) {
        let message =
          report.remoteUnhardenError ?? report.remoteCleanupError ?? "Löschen fehlgeschlagen.";
        if (report.hardeningRestored) {
          message += " Das SSH-Hardening wurde wiederhergestellt.";
        } else if (report.hardeningRestoreError) {
          message += ` Das SSH-Hardening konnte nicht wiederhergestellt werden (${report.hardeningRestoreError}); Passwort-Login ist auf dem Server wieder aktiv.`;
        }
        remoteError = message;
        return;
      }
      app.servers = app.servers.filter((s) => s.id !== server.id);
      if (app.selectedServerId === server.id) app.selectedServerId = null;
      await app.refresh();
      if (report.hardeningKept) {
        toast.info("Das SSH-Hardening bleibt auf dem Server aktiv.");
      }
      if (report.localKeyError) {
        toast.error(`SSH-Key konnte nicht vollständig gelöscht werden: ${report.localKeyError}`);
      } else if (removeUnusedKey && report.keyStillReferenced) {
        toast.info("Der SSH-Key bleibt erhalten, da ihn ein anderer Servereintrag verwendet.");
      }
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    void remove(false);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !submitting && onClose()} />

<div class="overlay">
  <form class="dialog" onsubmit={submit}>
    <h3>Server „{server.name}“ löschen?</h3>
    <p class="meta">{server.user}@{server.host}{server.port !== 22 ? `:${server.port}` : ""}</p>

    {#if hasKeyAuth}
      <label class="checkbox">
        <input type="checkbox" bind:checked={cleanupRemote} disabled={submitting} />
        SSH-Key auch vom Server entfernen
      </label>

      {#if cleanupRemote}
        <label>
          SSH-Login-Passwort für {server.user}@{server.host}
          <input type="password" bind:value={password} autocomplete="current-password" disabled={submitting} />
          <span class="hint">terminalox prüft den Passwort-Login, bevor es den SSH-Key entfernt.</span>
        </label>
        {#if usePassword}
          {#if server.hardened}
            <p class="body revert">
              terminalox macht zuerst das SSH-Hardening rückgängig. Schlägt der Passwort-Login danach fehl,
              wird das Hardening wiederhergestellt.
            </p>
          {/if}
        {:else}
          <label class="checkbox">
            <input type="checkbox" bind:checked={otherKeyConfirmed} disabled={submitting} />
            Kein Passwort – ich habe einen anderen SSH-Key für diesen Account
          </label>
          <span class="hint">
            Der Key wird nur entfernt, wenn danach noch ein anderer Key in authorized_keys steht.
            {#if server.hardened}Das SSH-Hardening bleibt aktiv.{/if}
          </span>
        {/if}
        {#if showSudo}
          <label>
            sudo-Passwort (falls erforderlich)
            <input type="password" bind:value={sudoPassword} autocomplete="current-password" disabled={submitting} />
          </label>
        {/if}
      {:else}
        <label class="checkbox">
          <input type="checkbox" bind:checked={deleteUnusedLocalKey} disabled={submitting} />
          Zugehörigen privaten SSH-Key lokal löschen, falls ihn kein anderer Servereintrag verwendet
        </label>
      {/if}
    {/if}

    {#if remoteError}<p class="error">{remoteError}</p>{/if}
    {#if error}<p class="error">{error}</p>{/if}

    <div class="buttons">
      <button type="button" onclick={onClose} disabled={submitting}>Abbrechen</button>
      {#if remoteError && cleanupRemote}
        <button type="button" onclick={() => remove(true)} disabled={submitting}>Nur lokal löschen</button>
      {/if}
      <button
        type="submit"
        class="danger"
        disabled={submitting || (hasKeyAuth && cleanupRemote && !usePassword && !otherKeyConfirmed)}
      >
        {submitting ? "…" : remoteError && cleanupRemote ? "Erneut versuchen" : "Löschen"}
      </button>
    </div>
  </form>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex; align-items: center; justify-content: center;
    z-index: 60;
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
  h3 { margin: 0 0 4px; font-size: 16px; }
  .meta { color: #9ca3af; margin: 0 0 14px; font-size: 12px; }
  .body { font-size: 13px; color: #d1d5db; line-height: 1.5; margin: 0 0 14px; }
  .body.revert { background: #0f2537; padding: 10px; border-radius: 6px; color: #93c5fd; font-size: 12px; }
  .hint { font-size: 11px; color: #9ca3af; display: block; margin-top: 4px; }
  label { display: block; font-size: 12px; color: #9ca3af; margin-bottom: 10px; }
  label.checkbox { display: flex; align-items: center; gap: 8px; color: #e5e7eb; font-size: 13px; }
  label.checkbox input { width: auto; }
  input[type="password"] {
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
  .error { color: #f87171; font-size: 13px; }
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
  button.danger { background: #7f1d1d; border-color: #7f1d1d; color: white; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
