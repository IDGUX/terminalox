<script lang="ts">
  import { api } from "$lib/api";
  import { app } from "$lib/stores/app.svelte";
  import { toast } from "$lib/stores/toast.svelte";
  import { isAppShortcut, shortcutLabel } from "$lib/shortcuts";
  import type { Folder, HardenReadiness, Server, Uuid } from "$lib/types";

  let {
    onAdd,
    onEdit,
    onDuplicate,
    onDeploy,
    onHarden,
    onDelete,
    onExport,
    onImport,
  }: {
    onAdd: (folderId?: Uuid | null) => void;
    onEdit: (s: Server) => void;
    onDuplicate: (s: Server) => void;
    onDeploy: (s: Server) => void;
    onHarden: (s: Server, r: HardenReadiness & { status: "ready" }) => void;
    onDelete: (s: Server) => void;
    onExport: () => void;
    onImport: () => void;
  } = $props();

  type ReadinessState =
    | { kind: "idle" }
    | { kind: "loading" }
    | { kind: "done"; value: HardenReadiness }
    | { kind: "error"; message: string };

  let readinessByServer = $state<Record<Uuid, ReadinessState>>({});

  // ---------- Ordner ----------

  const COLLAPSED_KEY = "terminalox.collapsedFolders";

  function loadCollapsed(): Record<Uuid, boolean> {
    try {
      return JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? "{}");
    } catch {
      return {};
    }
  }

  let collapsed = $state<Record<Uuid, boolean>>(loadCollapsed());

  function toggleFolder(id: Uuid) {
    collapsed = { ...collapsed, [id]: !collapsed[id] };
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify(collapsed));
    } catch {
      // Nur Komfort — ohne Storage klappt eben alles wieder auf.
    }
  }

  const sortedFolders = $derived(
    [...app.folders].sort((a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
    ),
  );
  const folderIds = $derived(new Set(app.folders.map((f) => f.id)));

  function serversIn(folderId: Uuid): Server[] {
    return app.servers.filter((s) => s.folder_id === folderId);
  }

  // ---------- Suche ----------

  let query = $state("");
  let searchInput = $state<HTMLInputElement>();
  const q = $derived(query.trim().toLowerCase());

  function matches(s: Server): boolean {
    if (!q) return true;
    const folder = app.folders.find((f) => f.id === s.folder_id)?.name ?? "";
    return [s.name, s.host, s.user, folder].some((v) => v.toLowerCase().includes(q));
  }

  /** Server eines Ordners, die zur aktuellen Suche passen. */
  function visibleIn(folderId: Uuid): Server[] {
    return serversIn(folderId).filter(matches);
  }

  const unfiledServers = $derived(
    app.servers.filter(
      (s) => (!s.folder_id || !folderIds.has(s.folder_id)) && matches(s),
    ),
  );

  // Reihenfolge wie in der Sidebar angezeigt — für Enter = ersten Treffer öffnen.
  const visibleServers = $derived([
    ...sortedFolders.flatMap((f) => visibleIn(f.id)),
    ...unfiledServers,
  ]);

  // ---------- Zuletzt verwendet ----------

  const RECENT_COUNT = 3;
  const RECENT_KEY = "__recent";

  const recentServers = $derived(
    app.servers
      .filter((s) => s.last_used_at)
      .sort((a, b) => Date.parse(b.last_used_at!) - Date.parse(a.last_used_at!))
      .slice(0, RECENT_COUNT),
  );
  // Erst sinnvoll, wenn es mehr Server gibt, als die Liste zeigen würde.
  const showRecent = $derived(
    !q && app.servers.length > RECENT_COUNT && recentServers.length > 0,
  );

  function lastUsedLabel(s: Server): string {
    if (!s.last_used_at) return "Noch nie verbunden";
    const min = Math.round((Date.now() - Date.parse(s.last_used_at)) / 60000);
    if (min < 1) return "Zuletzt verbunden: gerade eben";
    if (min < 60) return `Zuletzt verbunden: vor ${min} Min.`;
    const h = Math.round(min / 60);
    if (h < 24) return `Zuletzt verbunden: vor ${h} Std.`;
    const d = Math.round(h / 24);
    return `Zuletzt verbunden: vor ${d} ${d === 1 ? "Tag" : "Tagen"}`;
  }

  function onGlobalKeydown(e: KeyboardEvent) {
    if (isAppShortcut(e, "p")) {
      e.preventDefault();
      e.stopPropagation();
      searchInput?.focus();
      searchInput?.select();
    }
  }

  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      query = "";
      searchInput?.blur();
    } else if (e.key === "Enter" && visibleServers.length > 0) {
      select(visibleServers[0]);
      query = "";
      searchInput?.blur();
    }
  }

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  // null = kein Eingabefeld offen
  let newFolderName = $state<string | null>(null);

  async function commitNewFolder() {
    const name = newFolderName?.trim();
    newFolderName = null;
    if (!name) return;
    try {
      await app.createFolder(name);
    } catch (e) {
      toast.error(String(e));
    }
  }

  let renamingId = $state<Uuid | null>(null);
  let renameValue = $state("");

  function startRename(f: Folder) {
    renamingId = f.id;
    renameValue = f.name;
  }

  async function commitRename() {
    const id = renamingId;
    const name = renameValue.trim();
    renamingId = null;
    if (!id || !name || name === app.folders.find((f) => f.id === id)?.name) return;
    try {
      await app.renameFolder(id, name);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function removeFolder(f: Folder) {
    const n = serversIn(f.id).length;
    if (
      n > 0 &&
      !confirm(
        `Ordner „${f.name}" löschen?\n\nDie ${n} Server darin werden NICHT gelöscht, sondern nach „Ohne Ordner" verschoben.`,
      )
    ) {
      return;
    }
    try {
      await app.deleteFolder(f.id);
      toast.success(`Ordner „${f.name}" gelöscht`);
    } catch (e) {
      toast.error(String(e));
    }
  }

  // ---------- Drag & Drop ----------

  type DropTarget = Uuid | "unfiled";

  let dragServerId = $state<Uuid | null>(null);
  let dropTarget = $state<DropTarget | null>(null);

  function onDragStart(e: DragEvent, s: Server) {
    dragServerId = s.id;
    if (e.dataTransfer) {
      e.dataTransfer.setData("text/plain", s.id);
      e.dataTransfer.effectAllowed = "move";
    }
  }

  function onDragEnd() {
    dragServerId = null;
    dropTarget = null;
  }

  function onDragOver(e: DragEvent, target: DropTarget) {
    if (!dragServerId) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    dropTarget = target;
  }

  function onDragLeave(e: DragEvent, target: DropTarget) {
    const el = e.currentTarget as HTMLElement;
    if (dropTarget === target && !el.contains(e.relatedTarget as Node | null)) {
      dropTarget = null;
    }
  }

  async function onDrop(e: DragEvent, target: DropTarget) {
    e.preventDefault();
    const id = dragServerId;
    onDragEnd();
    if (!id) return;
    const folderId = target === "unfiled" ? null : target;
    const s = app.servers.find((x) => x.id === id);
    if (!s || (s.folder_id ?? null) === folderId) return;
    try {
      await app.moveServer(id, folderId);
    } catch (err) {
      toast.error(String(err));
    }
  }

  function select(s: Server) {
    app.selectedServerId = s.id;
    ensureReadiness(s);
  }

  function ensureReadiness(s: Server) {
    if (s.auth.mode !== "key" || s.hardened) return;
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
    if (r.kind === "loading") return "Prüfe, ob Härten möglich ist …";
    if (r.kind === "error") return `Konnte Readiness nicht prüfen: ${r.message}`;
    if (r.kind === "done" && r.value.status === "unavailable") return r.value.reason;
    if (r.kind === "done" && r.value.status === "ready") {
      if (r.value.method === "root") return "Härten möglich (als root eingeloggt)";
      if (r.value.method === "sudo_nopasswd") return "Härten möglich (passwortloses sudo)";
      return "Härten möglich (sudo, sudo-Passwort wird einmalig abgefragt)";
    }
    return "Passwort-Login serverseitig deaktivieren";
  }

  function hardenButtonLabel(r: ReadinessState): string {
    if (r.kind === "loading") return "Prüfe …";
    if (r.kind === "done" && r.value.status === "ready") return "Server härten";
    return "Härten nicht verfügbar";
  }

  function onHardenClick(s: Server) {
    const r = currentReadiness(s);
    if (r.kind === "done" && r.value.status === "ready") {
      onHarden(s, r.value);
    } else if (r.kind === "done" && r.value.status === "unavailable") {
      toast.error(r.value.reason);
    } else if (r.kind === "error") {
      toast.error(r.message);
    }
  }

  function showReadinessInfo(s: Server) {
    const r = currentReadiness(s);
    if (r.kind === "done" && r.value.status === "unavailable") {
      toast.error(r.value.reason);
    } else if (r.kind === "error") {
      toast.error(r.message);
    }
  }

  type AuthState = "password" | "key-unhardened" | "hardened";

  function authState(s: Server): AuthState {
    if (s.hardened) return "hardened";
    if (s.auth.mode === "key") return "key-unhardened";
    return "password";
  }

  function authTitle(s: Server): string {
    const st = authState(s);
    if (st === "hardened") return "Gehärtet: nur Key-Login möglich, Passwort-Login aus";
    if (st === "key-unhardened")
      return 'Key-Auth aktiv — aber Passwort-Login ist auf dem Server NOCH möglich. „Server härten" empfohlen.';
    return "Passwort-Auth";
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
        if (r.effectivePasswordAuthOff === true) {
          toast.success(
            `✓ Härtung bestätigt — sshd liefert PasswordAuthentication=no auf ${s.name}.`,
          );
        } else {
          // Drop-in da, sudo-Check übersprungen
          toast.success(
            `✓ Drop-in vorhanden auf ${s.name}. Effektiv-Check übersprungen: ${
              r.effectiveCheckError ?? "unbekannt"
            }.`,
          );
        }
      } else {
        // Drift erkannt
        const detail = !r.dropinPresent
          ? "Drop-in fehlt auf dem Server."
          : !r.dropinContentIntact
            ? "Drop-in wurde verändert."
            : r.effectivePasswordAuthOff === false
              ? "Effektive sshd-Konfig erlaubt wieder Passwort-Login."
              : "Härtung ist nicht mehr aktiv.";
        toast.error(`⚠ Härtung nicht mehr intakt: ${detail}`);
      }
    } catch (e) {
      toast.error(`Verify fehlgeschlagen: ${e}`);
    } finally {
      verifyingId = null;
    }
  }
</script>

<svelte:window onkeydowncapture={onGlobalKeydown} />

<aside class="sidebar">
  <header>
    <h2>terminalox</h2>
    <div class="header-buttons">
      <button
        class="add"
        onclick={() => (newFolderName = "")}
        title="Neuer Ordner"
        aria-label="Neuer Ordner">📁</button>
      <button class="add" onclick={() => onAdd()} title="Neuer Server">+</button>
    </div>
  </header>

  {#if app.servers.length > 0}
    <div class="search">
      <input
        type="search"
        placeholder={`Suchen …  (${shortcutLabel("p")})`}
        aria-label="Server suchen"
        bind:value={query}
        bind:this={searchInput}
        onkeydown={onSearchKeydown}
      />
    </div>
  {/if}

  {#snippet serverItem(s: Server)}
    <li
      class="server"
      class:active={app.selectedServerId === s.id}
      class:dragging={dragServerId === s.id}
      style:box-shadow={s.color ? `inset 3px 0 0 ${s.color}` : undefined}
      draggable="true"
      ondragstart={(e) => onDragStart(e, s)}
      ondragend={onDragEnd}
      onclick={() => select(s)}
    >
      <div class="row">
        <span class="auth auth-{authState(s)}" title={authTitle(s)}>
          {#if authState(s) === "hardened"}🛡
          {:else if authState(s) === "key-unhardened"}🔑<span class="warn-dot" aria-hidden="true">!</span>
          {:else}🔓{/if}
        </span>
        <span class="name">{s.name}</span>
      </div>
      <div class="meta" title={lastUsedLabel(s)}>
        {s.user}@{s.host}{s.port !== 22 ? `:${s.port}` : ""}
      </div>
    </li>
  {/snippet}

  <div class="tree">
    {#if newFolderName !== null}
      <div class="folder-head editing">
        <span class="chevron">📁</span>
        <input
          class="folder-input"
          placeholder="Ordnername, z.B. Hetzner"
          bind:value={newFolderName}
          use:focusSelect
          onkeydown={(e) => {
            if (e.key === "Enter") commitNewFolder();
            if (e.key === "Escape") newFolderName = null;
          }}
          onblur={commitNewFolder}
        />
      </div>
    {/if}

    {#if app.servers.length === 0 && app.folders.length === 0}
      <p class="empty">Noch keine Server. Klick auf <strong>+</strong>.</p>
    {/if}

    {#if showRecent}
      {@const open = !collapsed[RECENT_KEY]}
      <section class="folder recent" aria-label="Zuletzt verwendet">
        <div
          class="folder-head"
          role="button"
          tabindex="0"
          aria-expanded={open}
          onclick={() => toggleFolder(RECENT_KEY)}
          onkeydown={(e) => e.key === "Enter" && toggleFolder(RECENT_KEY)}
        >
          <span class="chevron">{open ? "▾" : "▸"}</span>
          <span class="folder-name">Zuletzt verwendet</span>
        </div>
        {#if open}
          <ul class="servers nested">
            {#each recentServers as s (s.id)}
              {@render serverItem(s)}
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if q && visibleServers.length === 0}
      <p class="empty">Keine Treffer für „{query.trim()}".</p>
    {/if}

    {#each sortedFolders as f (f.id)}
      {@const list = visibleIn(f.id)}
      {@const open = !!q || !collapsed[f.id]}
      {#if !q || list.length > 0}
      <section
        class="folder"
        class:drop={dropTarget === f.id}
        aria-label="Ordner {f.name}"
        ondragover={(e) => onDragOver(e, f.id)}
        ondragleave={(e) => onDragLeave(e, f.id)}
        ondrop={(e) => onDrop(e, f.id)}
      >
        <div
          class="folder-head"
          role="button"
          tabindex="0"
          aria-expanded={open}
          onclick={() => renamingId !== f.id && toggleFolder(f.id)}
          ondblclick={() => startRename(f)}
          onkeydown={(e) => e.key === "Enter" && e.target === e.currentTarget && toggleFolder(f.id)}
        >
          <span class="chevron">{open ? "▾" : "▸"}</span>
          {#if renamingId === f.id}
            <input
              class="folder-input"
              bind:value={renameValue}
              use:focusSelect
              onclick={(e) => e.stopPropagation()}
              ondblclick={(e) => e.stopPropagation()}
              onkeydown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter") commitRename();
                if (e.key === "Escape") renamingId = null;
              }}
              onblur={commitRename}
            />
          {:else}
            <span class="folder-name" title={f.name}>{f.name}</span>
            <span class="count">{list.length}</span>
            <span class="folder-actions">
              <button
                title={`Server in „${f.name}“ hinzufügen`}
                onclick={(e) => { e.stopPropagation(); onAdd(f.id); }}>+</button>
              <button
                title="Umbenennen"
                onclick={(e) => { e.stopPropagation(); startRename(f); }}>✎</button>
              <button
                class="del"
                title="Ordner löschen (Server bleiben erhalten)"
                onclick={(e) => { e.stopPropagation(); removeFolder(f); }}>✕</button>
            </span>
          {/if}
        </div>
        {#if open}
          <ul class="servers nested">
            {#each list as s (s.id)}
              {@render serverItem(s)}
            {:else}
              <li class="folder-empty">Leer — Server hierher ziehen</li>
            {/each}
          </ul>
        {/if}
      </section>
      {/if}
    {/each}

    {#if app.folders.length > 0}
      {#if unfiledServers.length > 0 || dragServerId}
        <section
          class="folder unfiled"
          class:drop={dropTarget === "unfiled"}
          aria-label="Ohne Ordner"
          ondragover={(e) => onDragOver(e, "unfiled")}
          ondragleave={(e) => onDragLeave(e, "unfiled")}
          ondrop={(e) => onDrop(e, "unfiled")}
        >
          <div class="folder-head static">
            <span class="folder-name">Ohne Ordner</span>
            <span class="count">{unfiledServers.length}</span>
          </div>
          <ul class="servers nested">
            {#each unfiledServers as s (s.id)}
              {@render serverItem(s)}
            {:else}
              <li class="folder-empty">Hierher ziehen, um aus dem Ordner zu nehmen</li>
            {/each}
          </ul>
        </section>
      {/if}
    {:else}
      <ul class="servers">
        {#each unfiledServers as s (s.id)}
          {@render serverItem(s)}
        {/each}
      </ul>
    {/if}
  </div>

  {#if app.selectedServer}
    {@const sel = app.selectedServer}
    {@const rd = currentReadiness(sel)}
    <div class="actions">
      <button onclick={() => onDeploy(sel)}>SSH-Key deployen</button>
      {#if sel.auth.mode === "key" && !sel.hardened}
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
              title="Warum ist Härten nicht möglich?"
              aria-label="Details anzeigen"
            >
              ⓘ
            </button>
          {/if}
        </div>
      {/if}
      {#if sel.auth.mode === "key" && sel.hardened}
        <button
          class="ghost-action"
          disabled={verifyingId === sel.id}
          onclick={() => verifyHardening(sel)}
          title="Prüfen, ob das Drop-in noch da ist und sshd Passwort-Login weiterhin abweist."
        >
          {verifyingId === sel.id ? "Prüfe …" : "🛡 Härtung prüfen"}
        </button>
      {/if}
      <div class="split-row">
        <button onclick={() => onEdit(sel)}>Bearbeiten</button>
        <button onclick={() => onDuplicate(sel)} title="Neuen Server mit denselben Daten anlegen">
          Duplizieren
        </button>
      </div>
      <button class="danger" onclick={() => onDelete(sel)}>
        Löschen…
      </button>
    </div>
  {/if}

  <footer>
    <button
      class="ghost"
      onclick={async () => {
        try {
          const p = await app.syncSshConfig();
          toast.success(`~/.ssh/config geschrieben (${p})`);
        } catch (e) {
          toast.error(String(e));
        }
      }}
    >
      ~/.ssh/config schreiben
    </button>
    <div class="backup-row">
      <button class="ghost-half" onclick={onExport} title="Backup exportieren">↑ Export</button>
      <button class="ghost-half" onclick={onImport} title="Backup importieren">↓ Import</button>
    </div>
  </footer>
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 260px;
    min-width: 260px;
    background: #1a1d23;
    border-right: 1px solid #2a2e37;
    color: #d1d5db;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    border-bottom: 1px solid #2a2e37;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  button.add {
    background: transparent;
    border: 1px solid #3a3f4b;
    color: #d1d5db;
    border-radius: 6px;
    width: 28px;
    height: 28px;
    cursor: pointer;
    font-size: 18px;
  }
  button.add:hover { background: #2a2e37; }
  .header-buttons { display: flex; gap: 6px; }
  .header-buttons button.add:first-child { font-size: 13px; }

  .search { padding: 8px 8px 2px; }
  .search input {
    width: 100%;
    box-sizing: border-box;
    background: #11141a;
    border: 1px solid #2a2e37;
    color: #e5e7eb;
    border-radius: 6px;
    padding: 6px 9px;
    font-size: 13px;
    outline: none;
  }
  .search input:focus { border-color: #3b82f6; }
  .search input::placeholder { color: #6b7280; }

  .tree {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
  }
  ul.servers {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  ul.servers.nested { padding-left: 10px; }

  .folder {
    border-radius: 6px;
    margin-bottom: 4px;
    border: 1px dashed transparent;
  }
  .folder.drop { border-color: #3b82f6; background: rgba(59, 130, 246, 0.08); }
  .folder-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    color: #9ca3af;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    user-select: none;
  }
  .folder-head:hover { background: #232732; }
  .folder-head.static { cursor: default; }
  .folder-head.static:hover { background: transparent; }
  .folder-head.editing { margin-bottom: 4px; }
  .chevron { width: 12px; flex: 0 0 12px; font-size: 11px; text-align: center; }
  .folder-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count {
    font-weight: 500;
    color: #6b7280;
    font-size: 11px;
  }
  .folder-actions { display: none; gap: 2px; }
  .folder-head:hover .folder-actions,
  .folder-head:focus-within .folder-actions { display: flex; }
  .folder-head:hover .count { display: none; }
  .folder-actions button {
    background: transparent;
    border: none;
    color: #9ca3af;
    cursor: pointer;
    font-size: 12px;
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 4px;
  }
  .folder-actions button:hover { background: #3a3f4b; color: #e5e7eb; }
  .folder-actions button.del:hover { background: #7f1d1d; }
  .folder-input {
    flex: 1;
    min-width: 0;
    background: #11141a;
    border: 1px solid #3b82f6;
    color: #e5e7eb;
    border-radius: 4px;
    padding: 3px 6px;
    font-size: 13px;
    text-transform: none;
    letter-spacing: normal;
    font-weight: 400;
    outline: none;
  }
  .folder-empty { color: #4b5563; font-size: 12px; padding: 4px 10px 8px; font-style: italic; }
  li.server.dragging { opacity: 0.4; }
  li.server {
    padding: 8px 10px;
    border-radius: 6px;
    cursor: pointer;
    margin-bottom: 2px;
  }
  li.server:hover { background: #232732; }
  li.server.active { background: #2d3440; }
  .empty { color: #6b7280; padding: 12px; font-size: 13px; margin: 0; }
  .row { display: flex; align-items: center; gap: 8px; }
  .auth { font-size: 13px; }
  .name { font-weight: 500; }
  .meta { color: #6b7280; font-size: 12px; margin-top: 2px; margin-left: 22px; }

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
  .folder.recent { border-bottom: 1px solid #2a2e37; border-radius: 6px 6px 0 0; padding-bottom: 4px; }
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

  .auth {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 20px;
    position: relative;
    font-size: 14px;
    line-height: 1;
  }
  .auth-hardened { filter: drop-shadow(0 0 3px rgba(52, 211, 153, 0.4)); }
  .auth-key-unhardened { position: relative; }
  .warn-dot {
    position: absolute;
    top: -4px;
    right: -4px;
    background: #f59e0b;
    color: #1a1d23;
    font-size: 9px;
    font-weight: 800;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid #1a1d23;
    line-height: 1;
  }

  footer { padding: 10px; border-top: 1px solid #2a2e37; }
  button.ghost {
    width: 100%;
    background: transparent;
    color: #9ca3af;
    border: 1px solid #2a2e37;
    padding: 6px;
    border-radius: 6px;
    font-size: 12px;
    cursor: pointer;
  }
  button.ghost:hover { background: #232732; }

  .backup-row { display: flex; gap: 6px; margin-top: 6px; }
  button.ghost-half {
    flex: 1;
    background: transparent;
    color: #9ca3af;
    border: 1px solid #2a2e37;
    padding: 6px;
    border-radius: 6px;
    font-size: 11px;
    cursor: pointer;
  }
  button.ghost-half:hover { background: #232732; color: #e5e7eb; }
</style>
