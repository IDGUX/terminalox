<script lang="ts">
  import { invoke } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import type { Server, Uuid } from "$lib/types";

  let { server, onClose }: { server: Server; onClose: () => void } = $props();

  interface DeleteReport {
    serverRemoved: boolean;
    remoteUnhardenAttempted: boolean;
    remoteUnhardenSucceeded: boolean;
    remoteUnhardenError: string | null;
    remoteCleanupAttempted: boolean;
    remoteCleanupSucceeded: boolean;
    remoteCleanupError: string | null;
    localKeyRemoved: boolean;
    keyStillReferenced: boolean;
  }

  let cleanupRemote = $state(true);
  let password = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let needsForcePrompt = $state(false);
  let needsSudoRetry = $state(false);
  let remoteErrorMsg = $state<string | null>(null);

  const isRoot = server.user === "root";
  const hasKeyAuth = server.auth.mode === "key";

  // Login-Passwort: nur wirklich nötig bei Passwort-Auth (sonst loggt der Key ein).
  const needsCleanupPassword = $derived(cleanupRemote && server.auth.mode === "password");
  // Sudo-Passwort beim Unharden: für root und passwordless-sudo unbenutzt.
  // Wir zeigen das Feld optional nur für non-root + key-auth + hardened.
  const showsOptionalSudoField = $derived(
    cleanupRemote && server.hardened && hasKeyAuth && !isRoot,
  );
  const showPasswordField = $derived(needsCleanupPassword || showsOptionalSudoField);
  const passwordRequired = $derived(needsCleanupPassword);

  function isSudoAuthError(msg: string | null): boolean {
    if (!msg) return false;
    return msg.includes("sudo_auth_failed") || msg.includes("sudo_password_required");
  }

  async function callDelete(force: boolean) {
    const trimmed = password.trim();
    const opts: {
      id: Uuid;
      cleanupRemote: boolean;
      password: string | null;
      sudoPassword: string | null;
      force: boolean;
    } = {
      id: server.id,
      cleanupRemote,
      password: needsCleanupPassword ? password : null,
      // sudoPassword: nur senden, falls Feld sichtbar UND etwas eingegeben.
      // Script ignoriert es ohnehin bei root/NOPASSWD sudo.
      sudoPassword: showsOptionalSudoField && trimmed.length > 0 ? password : null,
      force,
    };
    return (await invoke<DeleteReport>("delete_server_full", { opts })) as DeleteReport;
  }

  async function finish() {
    // Refresh local store view and selection.
    app.servers = app.servers.filter((s) => s.id !== server.id);
    if (app.selectedServerId === server.id) app.selectedServerId = null;
    await app.refresh();
    onClose();
  }

  async function submit(e: Event) {
    e.preventDefault();
    error = null;
    remoteErrorMsg = null;
    submitting = true;
    try {
      const report = await callDelete(false);
      if (!report.serverRemoved) {
        if (report.remoteUnhardenAttempted && !report.remoteUnhardenSucceeded) {
          const errMsg = report.remoteUnhardenError ?? "unbekannt";
          // Bei sudo-Auth-Fehler: kein "Force"-Pfad, sondern Passwort-Retry.
          if (isSudoAuthError(errMsg) && hasKeyAuth) {
            remoteErrorMsg =
              "Der Remote-User braucht ein sudo-Passwort, um das Hardening rückgängig zu machen.";
            password = "";
            needsSudoRetry = true;
            return;
          }
          remoteErrorMsg = "Unharden fehlgeschlagen: " + errMsg;
          needsForcePrompt = true;
          return;
        }
        if (report.remoteCleanupAttempted && !report.remoteCleanupSucceeded) {
          remoteErrorMsg =
            "Remote-Cleanup fehlgeschlagen: " + (report.remoteCleanupError ?? "unbekannt");
          needsForcePrompt = true;
          return;
        }
      }
      await finish();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }

  async function retryWithSudoPassword(e: Event) {
    e.preventDefault();
    if (!password.trim()) return;
    error = null;
    submitting = true;
    try {
      const opts = {
        id: server.id,
        cleanupRemote,
        password: null,
        sudoPassword: password,
        force: false,
      };
      const report = await invoke<DeleteReport>("delete_server_full", { opts });
      if (!report.serverRemoved) {
        const errMsg =
          report.remoteUnhardenError ?? report.remoteCleanupError ?? "unbekannt";
        // Falls Passwort wieder falsch: drin bleiben, Hinweis zeigen.
        if (isSudoAuthError(errMsg)) {
          error = "Passwort wurde von sudo abgelehnt.";
          return;
        }
        remoteErrorMsg = "Remote-Cleanup fehlgeschlagen: " + errMsg;
        needsSudoRetry = false;
        needsForcePrompt = true;
        return;
      }
      await finish();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }

  async function forceDelete() {
    submitting = true;
    error = null;
    try {
      await callDelete(true);
      await finish();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && !(submitting) && onClose()}
/>

<!-- Kein Schließen per Klick daneben: ein verirrter Klick würde alle Eingaben verwerfen. -->
<div class="overlay">
  <form class="dialog" onsubmit={submit}>
    <h3>Server „{server.name}" löschen?</h3>
    <p class="meta">{server.user}@{server.host}{server.port !== 22 ? `:${server.port}` : ""}</p>

    {#if needsSudoRetry}
      <p class="body">
        {remoteErrorMsg}
      </p>
      <label>
        sudo-Passwort für {server.user}@{server.host}
        <input
          type="password"
          bind:value={password}
          autocomplete="current-password"
          autofocus
        />
        <span class="hint">Wird nicht gespeichert.</span>
      </label>

      {#if error}<p class="error">{error}</p>{/if}

      <div class="buttons">
        <button type="button" onclick={onClose}>Abbrechen</button>
        <button
          type="button"
          class="danger"
          disabled={submitting || !password.trim()}
          onclick={retryWithSudoPassword}
        >
          {submitting ? "…" : "Erneut versuchen"}
        </button>
      </div>
    {:else if !needsForcePrompt}
      <p class="body">
        Lokal werden Server-Eintrag und — falls kein anderer Server ihn nutzt —
        der zugehörige SSH-Key (Datei in <code>~/.ssh/</code> und Eintrag) entfernt.
        <code>~/.ssh/config</code> wird aktualisiert.
      </p>

      {#if server.hardened && cleanupRemote}
        <p class="body revert">
          🛡 Dieser Server ist gehärtet. Beim Remote-Cleanup wird zuerst das Drop-in
          <code>/etc/ssh/sshd_config.d/00-terminalox.conf</code> gelöscht und
          <code>sshd</code> reloaded — damit ist Passwort-Login wieder möglich.
          {#if isRoot}
            Als <code>root</code> ist dafür kein Passwort nötig.
          {/if}
        </p>
      {/if}

      {#if server.auth.mode === "key" || server.auth.mode === "password"}
        <label class="checkbox">
          <input type="checkbox" bind:checked={cleanupRemote} />
          Public Key auch vom Remote-Host entfernen <span class="muted">(empfohlen)</span>
        </label>
      {/if}

      {#if showPasswordField}
        <label>
          {#if needsCleanupPassword}
            Passwort für {server.user}@{server.host}
          {:else}
            sudo-Passwort <span class="muted">(optional)</span>
          {/if}
          <input
            type="password"
            bind:value={password}
            required={passwordRequired}
            autocomplete="current-password"
          />
          <span class="hint">
            {#if needsCleanupPassword && showsOptionalSudoField}
              Für den Cleanup-Login und — falls nötig — als sudo-Passwort beim Revert.
            {:else if needsCleanupPassword}
              Nur einmal für den Cleanup-Connect.
            {:else}
              Nur eingeben, falls der Remote-User <code>sudo</code> mit Passwort braucht.
              Bei passwordless sudo leer lassen — wird bei Bedarf erneut abgefragt.
            {/if}
            Wird nicht gespeichert.
          </span>
        </label>
      {/if}

      {#if hasKeyAuth && cleanupRemote && !server.hardened}
        <p class="hint">
          Cleanup läuft über den bestehenden Key. Kein Passwort nötig.
        </p>
      {/if}

      {#if error}<p class="error">{error}</p>{/if}

      <div class="buttons">
        <button type="button" onclick={onClose}>Abbrechen</button>
        <button type="submit" class="danger" disabled={submitting}>
          {submitting ? "…" : "Löschen"}
        </button>
      </div>
    {:else}
      <p class="body error">
        Remote-Cleanup ist fehlgeschlagen:<br />
        <code>{remoteErrorMsg}</code>
      </p>
      <p class="body">
        Der Public Key bleibt dann in <code>~/.ssh/authorized_keys</code> auf dem Server
        liegen. Willst du trotzdem lokal löschen?
      </p>

      {#if error}<p class="error">{error}</p>{/if}

      <div class="buttons">
        <button type="button" onclick={onClose}>Abbrechen</button>
        <button type="button" class="danger" disabled={submitting} onclick={forceDelete}>
          {submitting ? "…" : "Nur lokal löschen"}
        </button>
      </div>
    {/if}
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
  .body.error { color: #fca5a5; }
  .body.revert { background: #0f2537; padding: 10px; border-radius: 6px; color: #93c5fd; font-size: 12px; }
  .muted { color: #6b7280; }
  .hint { font-size: 11px; color: #9ca3af; display: block; margin-top: 4px; }
  code { background: #11141a; padding: 1px 5px; border-radius: 3px; font-size: 12px; }
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
