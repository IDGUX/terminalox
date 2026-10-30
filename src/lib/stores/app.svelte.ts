import { api } from "../api";
import type { Folder, KeyInfo, Server, Uuid } from "../types";

export type View = "servers" | "keys";

class AppStore {
  servers = $state<Server[]>([]);
  keys = $state<KeyInfo[]>([]);
  folders = $state<Folder[]>([]);
  selectedServerId = $state<Uuid | null>(null);
  selectedKeyId = $state<Uuid | null>(null);
  activeSessionId = $state<Uuid | null>(null);
  view = $state<View>("servers");

  get selectedServer(): Server | null {
    return this.servers.find((s) => s.id === this.selectedServerId) ?? null;
  }

  get selectedKey(): KeyInfo | null {
    return this.keys.find((k) => k.id === this.selectedKeyId) ?? null;
  }

  async refresh() {
    const [servers, keys, folders] = await Promise.all([
      api.listServers(),
      api.listKeys(),
      api.listFolders(),
    ]);
    this.servers = servers;
    this.keys = keys;
    this.folders = folders;
  }

  async createFolder(name: string): Promise<Folder> {
    const f = await api.createFolder(name);
    this.folders = [...this.folders, f];
    return f;
  }

  async renameFolder(id: Uuid, name: string) {
    const f = await api.renameFolder(id, name);
    this.folders = this.folders.map((x) => (x.id === id ? f : x));
  }

  async deleteFolder(id: Uuid) {
    await api.deleteFolder(id);
    this.folders = this.folders.filter((f) => f.id !== id);
    this.servers = this.servers.map((s) =>
      s.folder_id === id ? { ...s, folder_id: null } : s,
    );
  }

  /** Lokal nachziehen, was das Backend beim Öffnen einer Session speichert. */
  markUsed(id: Uuid) {
    const now = new Date().toISOString();
    this.servers = this.servers.map((s) =>
      s.id === id ? { ...s, last_used_at: now } : s,
    );
  }

  async moveServer(serverId: Uuid, folderId: Uuid | null) {
    const updated = await api.moveServer(serverId, folderId);
    this.servers = this.servers.map((s) => (s.id === serverId ? updated : s));
  }

  async addServer(
    ...args: Parameters<typeof api.addServer>
  ): Promise<Server> {
    const srv = await api.addServer(...args);
    this.servers = [...this.servers, srv];
    return srv;
  }

  async deleteServer(id: Uuid) {
    await api.deleteServer(id);
    this.servers = this.servers.filter((s) => s.id !== id);
    if (this.selectedServerId === id) {
      this.selectedServerId = null;
    }
  }

  async generateKey(name: string): Promise<KeyInfo> {
    const k = await api.generateKey(name);
    this.keys = [...this.keys, k];
    return k;
  }

  async importKey(sourcePath: string, name: string): Promise<KeyInfo> {
    const k = await api.importKey(sourcePath, name);
    this.keys = [...this.keys, k];
    return k;
  }

  async deleteKey(id: Uuid) {
    await api.deleteKey(id);
    this.keys = this.keys.filter((k) => k.id !== id);
    if (this.selectedKeyId === id) this.selectedKeyId = null;
  }

  async syncSshConfig() {
    return api.syncSshConfig();
  }
}

export const app = new AppStore();
