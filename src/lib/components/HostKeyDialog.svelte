<script lang="ts">
  import { hostKeyPrompt } from "$lib/stores/hostkey.svelte";
  import { t } from "$lib/i18n/index.svelte";

  const cur = $derived(hostKeyPrompt.current);
  const r = $derived(cur?.rejection ?? null);
  const target = $derived(r ? (r.port === 22 ? r.host : `${r.host}:${r.port}`) : "");

  // Dateiname des Host-Keys auf dem Server, für den Vergleichsbefehl.
  const keyFile = $derived.by(() => {
    const a = r?.algorithm ?? "";
    if (a.includes("ed25519")) return "ssh_host_ed25519_key.pub";
    if (a.startsWith("ecdsa")) return "ssh_host_ecdsa_key.pub";
    return "ssh_host_rsa_key.pub";
  });
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && r && hostKeyPrompt.reject()}
/>

{#if r}
  <div class="overlay">
    <div class="dialog" class:changed={r.changed} role="alertdialog" aria-modal="true">
      {#if r.changed}
        <h3>⚠ {t("hostkey.changedTitle")}</h3>
        <p>{t("hostkey.changedBody", { target })}</p>
        <p><strong>{t("hostkey.changedWarn")}</strong></p>
      {:else}
        <h3>{t("hostkey.newTitle", { target })}</h3>
        <p>{t("hostkey.newBody")}</p>
      {/if}

      <div class="fp">
        <span class="algo">{r.algorithm}</span>
        <code>{r.fingerprint}</code>
      </div>

      <p class="hint">
        {t("hostkey.compareHint")}<br />
        <code>ssh-keygen -lf /etc/ssh/{keyFile}</code><br />
        {t("hostkey.knownHostsHint")}
      </p>

      {#if hostKeyPrompt.error}<p class="error">{hostKeyPrompt.error}</p>{/if}

      <div class="buttons">
        {#if r.changed}
          <button
            class="danger"
            disabled={hostKeyPrompt.busy}
            onclick={() => hostKeyPrompt.trust()}>{t("hostkey.trustAnyway")}</button>
          <!-- svelte-ignore a11y_autofocus -->
          <button class="primary" autofocus onclick={() => hostKeyPrompt.reject()}>
            {t("common.cancel")}
          </button>
        {:else}
          <button onclick={() => hostKeyPrompt.reject()}>{t("common.cancel")}</button>
          <!-- svelte-ignore a11y_autofocus -->
          <button
            class="primary"
            autofocus
            disabled={hostKeyPrompt.busy}
            onclick={() => hostKeyPrompt.trust()}>{t("hostkey.trustConnect")}</button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
  }
  .dialog {
    background: #1f232b;
    color: #e5e7eb;
    border-radius: 10px;
    padding: 20px;
    width: 480px;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
    border: 1px solid #2a2e37;
  }
  .dialog.changed { border-color: #b91c1c; }
  h3 { margin: 0 0 12px; font-size: 16px; }
  .changed h3 { color: #f87171; }
  p { font-size: 13px; line-height: 1.5; margin: 0 0 10px; color: #d1d5db; }
  .fp {
    background: #11141a;
    border: 1px solid #2a2e37;
    border-radius: 6px;
    padding: 10px 12px;
    margin: 12px 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .algo { font-size: 11px; color: #9ca3af; }
  .fp code { font-size: 13px; color: #a7f3d0; word-break: break-all; }
  .hint { font-size: 12px; color: #9ca3af; }
  .hint code { background: #11141a; padding: 1px 4px; border-radius: 3px; font-size: 11px; }
  .error { color: #f87171; }
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
  button.danger { background: #7f1d1d; border-color: #7f1d1d; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
