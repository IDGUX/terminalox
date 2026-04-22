import { invoke } from "@tauri-apps/api/core";
import type {
  DeployReport,
  HardenReport,
  KeyInfo,
  KeyUsage,
  NewServerInput,
  Server,
  Uuid,
} from "./types";

export const api = {
  listServers: () => invoke<Server[]>("list_servers"),
  addServer: (input: NewServerInput) =>
    invoke<Server>("add_server", { input }),
  deleteServer: (id: Uuid) => invoke<void>("delete_server", { id }),
  updateServer: (input: { id: Uuid } & Partial<Pick<Server, "name" | "host" | "port" | "user">> & { color?: string | null }) =>
    invoke<Server>("update_server", { input }),

  listKeys: () => invoke<KeyInfo[]>("list_keys"),
  generateKey: (name: string) => invoke<KeyInfo>("generate_key", { name }),
  importKey: (sourcePath: string, name: string) =>
    invoke<KeyInfo>("import_key", { sourcePath, name }),
  serversUsingKey: (keyId: Uuid) =>
    invoke<KeyUsage>("servers_using_key", { keyId }),
  deleteKey: (id: Uuid) => invoke<void>("delete_key", { id }),

  openSession: (serverId: Uuid, password: string | null, cols: number, rows: number) =>
    invoke<Uuid>("open_session", { serverId, password, cols, rows }),
  writeSession: (sessionId: Uuid, dataB64: string) =>
    invoke<void>("write_session", { sessionId, dataB64 }),
  resizeSession: (sessionId: Uuid, cols: number, rows: number) =>
    invoke<void>("resize_session", { sessionId, cols, rows }),
  closeSession: (sessionId: Uuid) =>
    invoke<void>("close_session", { sessionId }),

  deployKey: (
    serverId: Uuid,
    keyId: Uuid,
    password: string,
    assignToServer: boolean
  ) =>
    invoke<DeployReport>("deploy_key", {
      serverId,
      keyId,
      password,
      assignToServer,
    }),

  hardenServer: (serverId: Uuid, sudoPassword: string) =>
    invoke<HardenReport>("harden_server", { serverId, sudoPassword }),

  syncSshConfig: () => invoke<string>("sync_ssh_config"),
};
