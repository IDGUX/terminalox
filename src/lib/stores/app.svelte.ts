import { api } from "../api";
import type { KeyInfo, Server, Uuid } from "../types";

export type View = "servers" | "keys";

class AppStore {
  servers = $state<Server[]>([]);
  keys = $state<KeyInfo[]>([]);
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
    const [servers, keys] = await Promise.all([
      api.listServers(),
      api.listKeys(),
    ]);
    this.servers = servers;
    this.keys = keys;
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
