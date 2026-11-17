<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { errorText, t, tm } from "$lib/i18n/index.svelte";
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
        toast.success(t("harden.toastOk", { name: server.name }));
      } else {
        toast.error(t("harden.toastIncomplete"));
      }
      await app.refresh();
    } catch (e) {
      error = errorText(e);
    } finally {
      submitting = false;
    }
  }

  function stepIcon(ok: boolean): string {
    return ok ? "✓" : "…";
  }

  function methodLabel(m: PrivilegeMethod): string {
    if (m === "root") return t("harden.methodRoot");
    if (m === "sudo_nopasswd") return t("harden.methodNopasswd");
    return t("harden.methodPassword");
  }
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && !(submitting) && onClose()}
/>

<!-- Kein Schließen per Klick daneben: ein verirrter Klick würde alle Eingaben verwerfen. -->
<div class="overlay">
  <form class="dialog" onsubmit={submit}>
    {#if !report}
      <h3>{t("harden.title", { name: server.name })}</h3>
      <p class="hint">{t("harden.intro")}</p>

      <p class="method">✓ {methodLabel(readiness.method)}</p>

      {#if needsPassword}
        <label>
          {t("harden.sudoFor", { target: `${server.user}@${server.host}` })}
          <input
            type="password"
            bind:value={sudoPassword}
            required
            autofocus
            autocomplete="current-password"
          />
          <span class="hint">{t("harden.sudoHint")}</span>
        </label>
      {/if}

      {#if error}<p class="error">{error}</p>{/if}

      <div class="buttons">
        <button type="button" onclick={onClose}>{t("common.cancel")}</button>
        <button
          type="submit"
          class="primary"
          disabled={submitting || (needsPassword && !sudoPassword)}
        >
          {submitting ? t("harden.running") : t("harden.submit")}
        </button>
      </div>
    {:else}
      <h3>
        {report.hardened ? `✓ ${t("deploy.hardened")}` : `⚠ ${t("harden.failed")}`} — {server.name}
      </h3>

      <ul class="steps">
        <li class:ok={report.keyAuthVerified}>
          {stepIcon(report.keyAuthVerified)} {t("harden.stepKeyAuth")}
        </li>
        <li class:ok={report.dropinInstalled}>
          {stepIcon(report.dropinInstalled)} {t("step.dropinInstalled")}
        </li>
        <li class:ok={report.sshdValidated}>
          {stepIcon(report.sshdValidated)} {t("step.sshdValidated")}
        </li>
        <li class:ok={report.sshdReloaded}>
          {stepIcon(report.sshdReloaded)} {t("step.sshdReloaded")}
        </li>
        <li class:ok={report.passwordAuthConfirmedDisabled}>
          {stepIcon(report.passwordAuthConfirmedDisabled)} {t("harden.stepConfirmed")}
        </li>
      </ul>

      {#if report.warnings.length > 0}
        <div class="warnings">
          {#each report.warnings as w}
            <p class="warning">⚠ {tm(w)}</p>
          {/each}
        </div>
      {/if}

      {#if report.hardened}
        <p class="success">{t("harden.success")}</p>
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
