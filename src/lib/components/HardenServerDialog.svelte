<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import type { HardenReadiness, HardenReport, PrivilegeMethod, Server } from "$lib/types";

  let { server, readiness, onClose }: {
    server: Server;
    readiness: HardenReadiness & { status: "ready" };
    onClose: () => void;
  } = $props();

  let sudoPassword = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let report = $state<HardenReport | null>(null);

  const needsPassword = readiness.method === "sudo_with_password";

  async function submit(e: Event) {
    e.preventDefault();
    error = null;
    report = null;
    submitting = true;
    try {
      report = await api.hardenServer(
        server.id,
        needsPassword ? sudoPassword : null,
      );
      if (report.hardened) {
        toast.success(`${server.name} ist jetzt gehärtet`);
      } else {
        toast.error(`Härten unvollständig — siehe Details`);
      }
      await app.refresh();
    } catch (e) {
      error = String(e);
    } finally {
      submitting = false;
    }
  }

  function stepIcon(ok: boolean): string {
    return ok ? "✓" : "…";
  }

  function methodLabel(m: PrivilegeMethod): string {
    if (m === "root") return "Als root eingeloggt — keine Eskalation nötig";
    if (m === "sudo_nopasswd") return "Passwortloses sudo verfügbar (NOPASSWD)";
    return "sudo ist verfügbar, aber braucht dein Passwort";
  }
</script>

<div class="overlay" onclick={onClose}>
  <form class="dialog" onclick={(e) => e.stopPropagation()} onsubmit={submit}>
    {#if !report}
      <h3>{server.name} härten</h3>
      <p class="hint">
        Setzt <code>PasswordAuthentication no</code> via Drop-in
        <code>/etc/ssh/sshd_config.d/00-terminalox.conf</code>, validiert mit
        <code>sshd -t</code>, reloaded <code>sshd</code> und verifiziert mit
        <code>sshd -T</code>. Pre-Flight hat geprüft, dass Key-Auth läuft — du kannst dich
        nicht aussperren. Bei jedem Fehler: vollständiger Rollback.
      </p>

      <p class="method">✓ {methodLabel(readiness.method)}</p>

      {#if needsPassword}
        <label>
          sudo-Passwort für {server.user}@{server.host}
          <input
            type="password"
            bind:value={sudoPassword}
            required
            autofocus
            autocomplete="current-password"
          />
          <span class="hint">Einmalig für <code>sudo -v</code>, wird nicht gespeichert.</span>
        </label>
      {/if}

      {#if error}<p class="error">{error}</p>{/if}

      <div class="buttons">
        <button type="button" onclick={onClose}>Abbrechen</button>
        <button
          type="submit"
          class="primary"
          disabled={submitting || (needsPassword && !sudoPassword)}
        >
          {submitting ? "Härte…" : "Härten"}
        </button>
      </div>
    {:else}
      <h3>
        {report.hardened ? "✓ Gehärtet" : "⚠ Härten fehlgeschlagen"} — {server.name}
      </h3>

      <ul class="steps">
        <li class:ok={report.keyAuthVerified}>
          {stepIcon(report.keyAuthVerified)} Key-Auth funktioniert
        </li>
        <li class:ok={report.dropinInstalled}>
          {stepIcon(report.dropinInstalled)} Drop-in <code>00-terminalox.conf</code> geschrieben
        </li>
        <li class:ok={report.sshdValidated}>
          {stepIcon(report.sshdValidated)} <code>sshd -t</code> validiert
        </li>
        <li class:ok={report.sshdReloaded}>
          {stepIcon(report.sshdReloaded)} <code>sshd</code> reloaded
        </li>
        <li class:ok={report.passwordAuthConfirmedDisabled}>
          {stepIcon(report.passwordAuthConfirmedDisabled)}
          <code>sshd -T</code> bestätigt Passwort-Login ist aus
        </li>
      </ul>

      {#if report.warnings.length > 0}
        <div class="warnings">
          {#each report.warnings as w}
            <p class="warning">⚠ {w}</p>
          {/each}
        </div>
      {/if}

      {#if report.hardened}
        <p class="success">Server ist jetzt abgesichert: nur noch Key-Login möglich.</p>
      {:else}
        <p class="warning-box">
          Rollback hat stattgefunden — Passwort-Login ist wieder so wie vorher.
          Key-Auth funktioniert weiter.
        </p>
      {/if}

      <div class="buttons">
        <button type="button" class="primary" onclick={onClose}>OK</button>
      </div>
    {/if}
  </form>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex; align-items: center; justify-content: center;
    z-index: 50;
  }
  .dialog {
    background: #1f232b;
    color: #e5e7eb;
    border-radius: 10px;
    padding: 20px;
    width: 500px;
    border: 1px solid #2a2e37;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
  }
  h3 { margin: 0 0 10px; font-size: 16px; }
  .hint { font-size: 13px; color: #9ca3af; margin: 0 0 14px; line-height: 1.5; }
  .method {
    font-size: 12px;
    color: #34d399;
    background: #052e26;
    padding: 8px 10px;
    border-radius: 6px;
    margin: 0 0 14px;
  }
  code { background: #11141a; padding: 1px 5px; border-radius: 3px; font-size: 12px; }
  label { display: block; font-size: 12px; color: #9ca3af; margin-bottom: 10px; }
  input {
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
  .hint { font-size: 11px; color: #9ca3af; display: block; margin-top: 4px; }

  .steps { list-style: none; margin: 0 0 14px; padding: 0; }
  .steps li {
    padding: 6px 10px;
    border-radius: 4px;
    margin-bottom: 3px;
    color: #6b7280;
    background: #161a22;
    font-size: 13px;
  }
  .steps li.ok { color: #34d399; background: #052e26; }

  .warnings { margin: 10px 0; }
  .warning { color: #fbbf24; font-size: 12px; margin: 4px 0; line-height: 1.4; }
  .warning-box {
    background: #2d1f12;
    color: #fbbf24;
    padding: 10px;
    border-radius: 6px;
    font-size: 13px;
    line-height: 1.4;
  }
  .success {
    background: #052e26;
    color: #34d399;
    padding: 10px;
    border-radius: 6px;
    font-size: 13px;
    line-height: 1.4;
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
  button.primary { background: #2563eb; border-color: #2563eb; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
