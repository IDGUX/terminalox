<script lang="ts">
  import { untrack } from "svelte";
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { t, tm } from "$lib/i18n/index.svelte";
  import type { HardenReadiness, Server, Uuid } from "$lib/types";

  let {
    server,
    onDeploy,
    onHarden,
    onEdit,
    onDuplicate,
    onDelete,
    onRotate,
  }: {
    server: Server;
    onDeploy: (s: Server) => void;
    onHarden: (s: Server, r: HardenReadiness & { status: "ready" }) => void;
    onEdit: (s: Server) => void;
    onDuplicate: (s: Server) => void;
    onDelete: (s: Server) => void;
    onRotate: (s: Server) => void;
  } = $props();

  type ReadinessState =
    | { kind: "idle" }
    | { kind: "loading" }
    | { kind: "done"; value: HardenReadiness }
    | { kind: "error"; message: string };

  let readinessByServer = $state<Record<Uuid, ReadinessState>>({});

  // Readiness wird beim Auswählen eines Servers einmal geprüft (und nach Fehlern erneut).
  $effect(() => {
    const s = server;
    untrack(() => ensureReadiness(s));
  });

  function ensureReadiness(s: Server) {
    if (s.auth.mode === "password" || s.hardened) return;
    const cur = readinessByServer[s.id];
    if (cur && cur.kind !== "error") return;
    readinessByServer = { ...readinessByServer, [s.id]: { kind: "loading" } };
    api
      .checkHardenReadiness(s.id)
      .then((value) => {
        readinessByServer = { ...readinessByServer, [s.id]: { kind: "done", value } };
      })
      .catch((e) => {
        readinessByServer = {
          ...readinessByServer,
          [s.id]: { kind: "error", message: String(e) },
        };
      });
  }

  function currentReadiness(s: Server | null): ReadinessState {
    if (!s) return { kind: "idle" };
    return readinessByServer[s.id] ?? { kind: "idle" };
  }

  function hardenButtonTitle(r: ReadinessState): string {
    if (r.kind === "loading") return t("actions.checkingReadiness");
    if (r.kind === "error") return t("actions.readinessError", { detail: r.message });
    if (r.kind === "done" && r.value.status === "unavailable") return tm(r.value.reason);
    if (r.kind === "done" && r.value.status === "ready") {
      if (r.value.method === "root") return t("actions.readyRoot");
      if (r.value.method === "sudo_nopasswd") return t("actions.readyNopasswd");
      return t("actions.readySudo");
    }
    return t("actions.hardenDefault");
  }

  function hardenButtonLabel(r: ReadinessState): string {
    if (r.kind === "loading") return t("common.checking");
    if (r.kind === "done" && r.value.status === "ready") return t("actions.harden");
    return t("actions.hardenUnavailable");
  }

  function onHardenClick(s: Server) {
    const r = currentReadiness(s);
    if (r.kind === "done" && r.value.status === "ready") {
      onHarden(s, r.value);
    } else if (r.kind === "done" && r.value.status === "unavailable") {
      toast.error(tm(r.value.reason));
    } else if (r.kind === "error") {
      toast.error(r.message);
    }
  }

  function showReadinessInfo(s: Server) {
    const r = currentReadiness(s);
    if (r.kind === "done" && r.value.status === "unavailable") {
      toast.error(tm(r.value.reason));
    } else if (r.kind === "error") {
      toast.error(r.message);
    }
  }

  let verifyingId = $state<Uuid | null>(null);

  async function verifyHardening(s: Server) {
    verifyingId = s.id;
    try {
      const r = await api.verifyServerHardening(s.id);
      // Lokalen Store nachziehen, falls Backend den Flag geändert hat.
      if (r.stateChanged) {
        await app.refresh();
      }
      if (r.isHardened) {
        toast.success(t("actions.verifyOk", { name: s.name }));
      } else {
        // Drift erkannt
        const detail = !r.dropinPresent
          ? t("actions.driftMissing")
          : !r.dropinContentIntact
            ? t("actions.driftChanged")
            : r.effectivePasswordAuthOff === false
              ? t("actions.driftPasswordOn")
              : r.effectiveCheckError
                ? tm(r.effectiveCheckError)
                : t("actions.driftUnchecked");
        toast.error(`⚠ ${t("actions.driftTitle", { detail })}`);
      }
    } catch (e) {
      toast.error(t("actions.verifyFailed", { detail: String(e) }));
    } finally {
      verifyingId = null;
    }
  }
</script>

{#if server}
  {@const sel = server}
  {@const rd = currentReadiness(sel)}
  <div class="actions">
    <button onclick={() => onDeploy(sel)}>{t("actions.deploy")}</button>
    {#if sel.auth.mode !== "password" && !sel.hardened}
      {@const ready = rd.kind === "done" && rd.value.status === "ready"}
      {@const showInfo =
        (rd.kind === "done" && rd.value.status === "unavailable") || rd.kind === "error"}
      <div class="harden-row">
        <button
          class="warn harden-btn"
          disabled={!ready}
          onclick={() => onHardenClick(sel)}
          title={hardenButtonTitle(rd)}
        >
          🛡 {hardenButtonLabel(rd)}
        </button>
        {#if showInfo}
          <button
            type="button"
            class="info-btn"
            onclick={() => showReadinessInfo(sel)}
            title={t("actions.whyUnavailable")}
          aria-label={t("actions.showDetails")}
          >
            ⓘ
          </button>
        {/if}
      </div>
    {/if}
    {#if sel.auth.mode !== "password" && sel.hardened}
      <button
        class="ghost-action"
        disabled={verifyingId === sel.id}
        onclick={() => verifyHardening(sel)}
        title={t("actions.verifyTitle")}
      >
        {verifyingId === sel.id ? t("common.checking") : `🛡 ${t("actions.verify")}`}
      </button>
    {/if}
    {#if sel.auth.mode === "key"}
      <button
        class="ghost-action neutral"
        onclick={() => onRotate(sel)}
        title={t("actions.rotateTitle")}
      >
        ⟳ {t("actions.rotate")}
      </button>
    {/if}
    <div class="split-row">
      <button onclick={() => onEdit(sel)}>{t("actions.edit")}</button>
      <button onclick={() => onDuplicate(sel)} title={t("actions.duplicateTitle")}>
        {t("actions.duplicate")}
      </button>
    </div>
    <button class="danger" onclick={() => onDelete(sel)}>
      {t("actions.delete")}
    </button>
  </div>
{/if}

<style>
  .actions { padding: 10px; border-top: 1px solid #2a2e37; display: flex; flex-direction: column; gap: 6px; }
  .actions button {
    background: #2563eb;
    color: white;
    border: none;
    padding: 8px 10px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  .actions button.danger { background: #7f1d1d; }
  .actions button.warn { background: #a16207; }
  .actions button.ghost-action {
    background: transparent;
    color: #a7f3d0;
    border: 1px solid #064e3b;
  }
  .actions button.ghost-action:hover { background: #052e26; filter: none; }
  .actions button.ghost-action:disabled { color: #6b7280; border-color: #2a2e37; }
  .actions button:hover { filter: brightness(1.1); }
  .actions button:disabled { opacity: 0.55; cursor: not-allowed; }
  .actions button:disabled:hover { filter: none; }

  .split-row { display: flex; gap: 6px; }
  .split-row button { flex: 1; }
  .harden-row { display: flex; gap: 6px; }
  .harden-row .harden-btn { flex: 1; }
  .harden-row .info-btn {
    flex: 0 0 32px;
    background: #2a2e37;
    color: #d1d5db;
    border: 1px solid #3a3f4b;
    padding: 0;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    border-radius: 6px;
  }
  .harden-row .info-btn:hover { background: #3a3f4b; filter: none; }

  .actions button.ghost-action.neutral { color: #c7d2fe; border-color: #312e81; }
  .actions button.ghost-action.neutral:hover { background: #1e1b4b; }
</style>
