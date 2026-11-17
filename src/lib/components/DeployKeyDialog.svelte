<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { errorText, t, tm } from "$lib/i18n/index.svelte";
  import type { DeployReport, Server } from "$lib/types";

  let { server, onClose }: { server: Server; onClose: () => void } = $props();

  let password = $state("");
  let keyChoice = $state<"new" | string>("new");
  let newKeyName = $state(server.name.replace(/[^a-zA-Z0-9_-]/g, "_"));
  let assignToServer = $state(true);
  let sudoDiffers = $state(false);
  let sudoPassword = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let report = $state<DeployReport | null>(null);

  async function submit(e: Event) {
    e.preventDefault();
    error = null;
    report = null;
    submitting = true;
    try {
      let keyId: string;
      if (keyChoice === "new") {
        const k = await app.generateKey(newKeyName.trim());
        keyId = k.id;
      } else {
        keyId = keyChoice;
      }
      const effectiveSudo = sudoDiffers && sudoPassword ? sudoPassword : null;
      report = await api.deployKey(
        server.id,
        keyId,
        password,
        assignToServer,
        effectiveSudo,
      );
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
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && !(submitting) && onClose()}
/>

<!-- Kein Schließen per Klick daneben: ein verirrter Klick würde alle Eingaben verwerfen. -->
<div class="overlay">
  <form class="dialog" onsubmit={submit}>
    {#if !report}
      <h3>{t("deploy.title", { name: server.name })}</h3>
      <p class="hint">{t("deploy.intro")}</p>

      <label>
        {t("deploy.chooseKey")}
        <select bind:value={keyChoice}>
          <option value="new">-- {t("deploy.newKey")} --</option>
          {#each app.keys as k (k.id)}
            <option value={k.id}>{k.name} ({k.algorithm})</option>
          {/each}
        </select>
      </label>

      {#if keyChoice === "new"}
        <label>
          {t("deploy.newKeyName")}
          <input bind:value={newKeyName} required />
        </label>
      {/if}

      <label>
        {t("deploy.password")}
        <input type="password" bind:value={password} required autofocus />
        <span class="hint">{t("deploy.passwordHint")}</span>
      </label>

      <label class="checkbox">
        <input type="checkbox" bind:checked={sudoDiffers} />
        {t("deploy.sudoDiffers")}
      </label>

      {#if sudoDiffers}
        <label>
          {t("deploy.sudoPassword")}
          <input type="password" bind:value={sudoPassword} />
        </label>
      {/if}

      <label class="checkbox">
        <input type="checkbox" bind:checked={assignToServer} />
        {t("deploy.assign")}
      </label>

      {#if error}<p class="error">{error}</p>{/if}

      <div class="buttons">
        <button type="button" onclick={onClose}>{t("common.cancel")}</button>
        <button type="submit" class="primary" disabled={submitting}>
          {submitting ? t("deploy.running") : t("deploy.submit")}
        </button>
      </div>
    {:else}
      <h3>
        {report.hardened ? `✓ ${t("deploy.hardened")}` : `⚠ ${t("deploy.partial")}`} —
        {server.name}
      </h3>

      <ul class="steps">
        <li class:ok={report.keyDeployed}>
          {stepIcon(report.keyDeployed)} {t("step.keyDeployed")}
        </li>
        <li class:ok={report.keyAuthVerified}>
          {stepIcon(report.keyAuthVerified)} {t("step.keyAuthVerified")}
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
          {stepIcon(report.passwordAuthConfirmedDisabled)} {t("step.passwordOff")}
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
        <p class="success">{t("deploy.successBody")}</p>
      {:else if report.keyDeployed}
        <p class="warning-box">{t("deploy.partialBody")}</p>
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
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
    border: 1px solid #2a2e37;
  }
  h3 { margin: 0 0 10px; font-size: 16px; }
  .hint { font-size: 13px; color: #9ca3af; margin: 0 0 14px; line-height: 1.5; }
  label { display: block; font-size: 12px; color: #9ca3af; margin-bottom: 10px; }
  label.checkbox { display: flex; align-items: center; gap: 8px; color: #e5e7eb; font-size: 13px; }
  label.checkbox input { width: auto; }
  input, select {
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
  code { background: #11141a; padding: 1px 5px; border-radius: 3px; font-size: 12px; }

  .steps {
    list-style: none;
    margin: 0 0 14px;
    padding: 0;
  }
  .steps li {
    padding: 6px 10px;
    border-radius: 4px;
    margin-bottom: 3px;
    color: #6b7280;
    background: #161a22;
    font-size: 13px;
  }
  .steps li.ok {
    color: #34d399;
    background: #052e26;
  }
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
