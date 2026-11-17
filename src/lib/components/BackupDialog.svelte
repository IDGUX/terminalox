<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { errorText, formatDateTime, t, tm, type Msg } from "$lib/i18n/index.svelte";

  let { mode, onClose }: { mode: "export" | "import"; onClose: () => void } = $props();

  // ---- Export state ----
  let useEncryption = $state(true);
  let passphrase = $state("");
  let passphrase2 = $state("");
  let exporting = $state(false);

  // ---- Import state ----
  let sourcePath = $state("");
  let importPassphrase = $state("");
  let needsPassphrase = $state(false);
  let preview = $state<InspectReport | null>(null);
  let replaceExisting = $state(false);
  let importing = $state(false);

  interface InspectReport {
    encrypted: boolean;
    version: number | null;
    exportedAt: string | null;
    sourcePlatform: string | null;
    servers: number;
    keys: number;
    needsPassphrase: boolean;
    serverNames: string[];
    keyNames: string[];
  }

  interface ImportReport {
    serversAdded: number;
    serversUpdated: number;
    serversSkipped: number;
    keysAdded: number;
    keysUpdated: number;
    keysSkipped: number;
    keyFileConflicts: string[];
    serversInvalid: { name: string; msg: Msg }[];
  }

  interface ExportReport {
    targetPath: string;
    encrypted: boolean;
    servers: number;
    keys: number;
  }

  let error = $state<string | null>(null);
  let result = $state<ImportReport | ExportReport | null>(null);

  async function doExport(e: Event) {
    e.preventDefault();
    error = null;
    if (useEncryption) {
      if (passphrase.length < 8) {
        error = t("backup.errShort");
        return;
      }
      if (passphrase !== passphrase2) {
        error = t("backup.errMismatch");
        return;
      }
    }
    const today = new Date().toISOString().slice(0, 10);
    const target = await saveDialog({
      title: t("backup.saveTitle"),
      defaultPath: `terminalox-backup-${today}.json`,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!target) return;
    exporting = true;
    try {
      const report = await invoke<ExportReport>("export_backup", {
        targetPath: target,
        passphrase: useEncryption ? passphrase : null,
      });
      result = report;
      toast.success(
        t(report.encrypted ? "backup.toastSavedEnc" : "backup.toastSaved", {
          servers: report.servers,
          keys: report.keys,
        }),
      );
    } catch (e) {
      error = errorText(e);
    } finally {
      exporting = false;
    }
  }

  async function pickImportFile() {
    const sel = await openDialog({
      multiple: false,
      directory: false,
      title: t("backup.pickTitle"),
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof sel === "string") {
      sourcePath = sel;
      await inspect();
    }
  }

  async function inspect() {
    error = null;
    preview = null;
    try {
      const rep = await invoke<InspectReport>("inspect_backup", {
        sourcePath,
        passphrase: importPassphrase || null,
      });
      if (rep.needsPassphrase) {
        needsPassphrase = true;
        preview = rep;
      } else {
        needsPassphrase = false;
        preview = rep;
      }
    } catch (e) {
      error = errorText(e);
    }
  }

  async function doImport() {
    if (!preview) return;
    if (preview.needsPassphrase) {
      error = t("backup.errNoPassphrase");
      return;
    }
    importing = true;
    error = null;
    try {
      const rep = await invoke<ImportReport>("import_backup", {
        opts: {
          sourcePath,
          passphrase: importPassphrase || null,
          replaceExisting,
        },
      });
      result = rep;
      await app.refresh();
      toast.success(
        t("backup.toastImported", {
          sa: rep.serversAdded,
          su: rep.serversUpdated,
          ka: rep.keysAdded,
          ku: rep.keysUpdated,
        }),
      );
    } catch (e) {
      error = errorText(e);
    } finally {
      importing = false;
    }
  }
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && !(exporting || importing) && onClose()}
/>

<!-- Kein Schließen per Klick daneben: ein verirrter Klick würde alle Eingaben verwerfen. -->
<div class="overlay">
  <div class="dialog">
    {#if mode === "export"}
      <h3>{t("backup.exportTitle")}</h3>

      {#if !result}
        <p class="hint">{t("backup.exportIntro", { servers: app.servers.length, keys: app.keys.length })}</p>

        <form onsubmit={doExport}>
          <label class="checkbox">
            <input type="checkbox" bind:checked={useEncryption} />
            <span>
              {t("backup.encrypt")} <em>({t("backup.recommended")})</em>
              <br /><span class="sub">{t("backup.encryptHint")}</span>
            </span>
          </label>

          {#if useEncryption}
            <label>
              {t("backup.passphraseMin")}
              <input type="password" bind:value={passphrase} autocomplete="new-password" />
            </label>
            <label>
              {t("backup.passphraseRepeat")}
              <input type="password" bind:value={passphrase2} autocomplete="new-password" />
            </label>
          {:else}
            <div class="danger-box">⚠ {t("backup.plainWarning")}</div>
          {/if}

          {#if error}<p class="error">{error}</p>{/if}

          <div class="buttons">
            <button type="button" onclick={onClose}>{t("common.cancel")}</button>
            <button type="submit" class="primary" disabled={exporting}>
              {exporting ? "…" : t("backup.saveAs")}
            </button>
          </div>
        </form>
      {:else}
        {@const r = result as ExportReport}
        <p class="success">
          ✓ {t("backup.saved")}<br /><code>{r.targetPath}</code>
        </p>
        <p class="hint">
          {t("backup.summary", { servers: r.servers, keys: r.keys })} —
          {r.encrypted ? t("backup.encryptedAes") : t("backup.unencrypted")}
        </p>
        <div class="buttons">
          <button class="primary" onclick={onClose}>OK</button>
        </div>
      {/if}
    {:else}
      <!-- IMPORT -->
      <h3>{t("backup.importTitle")}</h3>

      {#if !result}
        <p class="hint">{t("backup.importIntro")}</p>

        <label>
          {t("backup.file")}
          <div class="row">
            <input bind:value={sourcePath} placeholder={t("backup.filePlaceholder")} />
            <button type="button" onclick={pickImportFile}>{t("common.browse")}</button>
          </div>
        </label>

        {#if sourcePath && !preview && !error}
          <button type="button" onclick={inspect}>{t("backup.loadPreview")}</button>
        {/if}

        {#if preview?.encrypted}
          <label>
            {t("secret.passphrase")}
            <div class="row">
              <input type="password" bind:value={importPassphrase} autocomplete="current-password" />
              <button type="button" onclick={inspect}>{t("backup.check")}</button>
            </div>
          </label>
        {/if}

        {#if preview && !preview.needsPassphrase}
          <div class="preview">
            <h4>{t("backup.contents")}</h4>
            <p>
              {t("backup.meta", {
                version: preview.version ?? "?",
                date: preview.exportedAt ? formatDateTime(preview.exportedAt) : "?",
                platform: preview.sourcePlatform ?? "?",
              })}
              {#if preview.encrypted}· 🔒 {t("backup.encryptedShort")}{/if}
            </p>
            <p>
              <strong>{preview.servers}</strong> {t("backup.serversLabel")}: {preview.serverNames.join(", ") || "—"}
            </p>
            <p>
              <strong>{preview.keys}</strong> {t("backup.keysLabel")}: {preview.keyNames.join(", ") || "—"}
            </p>

            <label class="checkbox">
              <input type="checkbox" bind:checked={replaceExisting} />
              <span>{t("backup.replace")}</span>
            </label>
            <p class="sub">{t("backup.replaceHint")}</p>
          </div>
        {/if}

        {#if error}<p class="error">{error}</p>{/if}

        <div class="buttons">
          <button type="button" onclick={onClose}>{t("common.cancel")}</button>
          {#if preview && !preview.needsPassphrase}
            <button class="primary" disabled={importing} onclick={doImport}>
              {importing ? "…" : t("backup.import")}
            </button>
          {/if}
        </div>
      {:else}
        {@const r = result as ImportReport}
        <p class="success">✓ {t("backup.importDone")}</p>
        <ul class="result-list">
          <li>{t("backup.serversAdded")}: <strong>{r.serversAdded}</strong></li>
          <li>{t("backup.serversUpdated")}: <strong>{r.serversUpdated}</strong></li>
          <li>{t("backup.serversSkipped")}: <strong>{r.serversSkipped}</strong></li>
          <li>{t("backup.keysAdded")}: <strong>{r.keysAdded}</strong></li>
          <li>{t("backup.keysUpdated")}: <strong>{r.keysUpdated}</strong></li>
          <li>{t("backup.keysSkipped")}: <strong>{r.keysSkipped}</strong></li>
        </ul>
        {#if r.serversInvalid?.length > 0}
          <p class="warn">⚠ {t("backup.serversInvalid", { count: r.serversInvalid.length })}</p>
          <ul class="conflicts">
            {#each r.serversInvalid as p}
              <li>{p.name}: {tm(p.msg)}</li>
            {/each}
          </ul>
        {/if}
        {#if r.keyFileConflicts.length > 0}
          <p class="warn">⚠ {t("backup.conflicts", { count: r.keyFileConflicts.length })}</p>
          <ul class="conflicts">
            {#each r.keyFileConflicts as c}
              <li>{c}</li>
            {/each}
          </ul>
        {/if}
        <div class="buttons">
          <button class="primary" onclick={onClose}>OK</button>
        </div>
      {/if}
    {/if}
  </div>
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
    padding: 22px;
    width: 520px;
    max-height: 85vh;
    overflow-y: auto;
    border: 1px solid #2a2e37;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
  }
  h3 { margin: 0 0 12px; font-size: 16px; }
  h4 { margin: 0 0 6px; font-size: 13px; color: #9ca3af; text-transform: uppercase; letter-spacing: 0.04em; }
  .hint { font-size: 12px; color: #9ca3af; margin: 0 0 14px; line-height: 1.5; }
  .sub { font-size: 11px; color: #6b7280; margin: 4px 0 0; line-height: 1.4; display: block; }
  label { display: block; font-size: 12px; color: #9ca3af; margin-bottom: 12px; }
  label.checkbox { display: flex; align-items: flex-start; gap: 8px; color: #e5e7eb; font-size: 13px; margin-bottom: 14px; }
  label.checkbox input { width: auto; margin-top: 3px; }
  label.checkbox em { font-style: normal; color: #34d399; font-size: 11px; }
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
  .row { display: flex; gap: 8px; align-items: center; margin-top: 4px; }
  .row input { margin-top: 0; flex: 1; }
  .row button {
    background: #2a2e37;
    border: 1px solid #2a2e37;
    color: #e5e7eb;
    padding: 0 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    height: 36px;
    white-space: nowrap;
  }
  .danger-box {
    background: #2d1f12;
    color: #fbbf24;
    padding: 10px;
    border-radius: 6px;
    font-size: 12px;
    line-height: 1.4;
    margin-bottom: 14px;
  }
  .preview {
    background: #151923;
    padding: 12px 14px;
    border-radius: 6px;
    border: 1px solid #2a2e37;
    margin: 10px 0;
  }
  .preview p { margin: 4px 0; font-size: 13px; }
  .result-list {
    list-style: none;
    padding: 0;
    margin: 12px 0;
    background: #151923;
    border-radius: 6px;
    border: 1px solid #2a2e37;
  }
  .result-list li {
    padding: 6px 14px;
    font-size: 13px;
    display: flex;
    justify-content: space-between;
    border-bottom: 1px solid #1f232b;
  }
  .result-list li:last-child { border-bottom: none; }
  .conflicts {
    font-family: monospace;
    font-size: 11px;
    background: #151923;
    padding: 10px 14px;
    border-radius: 6px;
    color: #9ca3af;
    max-height: 120px;
    overflow-y: auto;
  }
  .error { color: #f87171; font-size: 13px; }
  .success { color: #34d399; font-size: 13px; margin: 0 0 10px; }
  .warn { color: #fbbf24; font-size: 12px; }
  code { background: #11141a; padding: 2px 6px; border-radius: 3px; font-size: 12px; color: #93c5fd; word-break: break-all; }
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
