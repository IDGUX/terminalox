import { invoke as rawInvoke } from "@tauri-apps/api/core";
import { hostKeyPrompt } from "./stores/hostkey.svelte";
import type {
  AuthMode,
  DeployReport,
  Folder,
  HardenReadiness,
  HardenReport,
  HostKeyRejection,
  KeyInfo,
  KeyUsage,
  NewServerInput,
  Server,
  Uuid,
  VerifyHardenReport,
} from "./types";

function hostKeyError(e: unknown): HostKeyRejection | null {
  if (e && typeof e === "object" && "hostKey" in e) {
    return (e as { hostKey: HostKeyRejection }).hostKey;
  }
  return null;
}

/** Wie `invoke`, aber bei unbekanntem/geändertem Host-Key wird der Nutzer gefragt
 *  und der Aufruf nach Bestätigung wiederholt. Der Server lehnt vor jeder
 *  Authentifizierung ab, die Wiederholung ist also unkritisch. */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await rawInvoke<T>(cmd, args);
  } catch (e) {
    const hk = hostKeyError(e);
    if (!hk) throw e;
    if (!(await hostKeyPrompt.ask(hk))) {
      throw `Verbindung zu ${hk.host} abgebrochen: Host-Key nicht bestätigt.`;
    }
    return invoke<T>(cmd, args);
  }
}

export const api = {
  listServers: () => invoke<Server[]>("list_servers"),
  addServer: (input: NewServerInput) =>
    invoke<Server>("add_server", { input }),
  deleteServer: (id: Uuid) => invoke<void>("delete_server", { id }),
  updateServer: (input: { id: Uuid } & Partial<Pick<Server, "name" | "host" | "port" | "user">> & { color?: string | null; auth?: AuthMode }) =>
    invoke<Server>("update_server", { input }),

  listFolders: () => invoke<Folder[]>("list_folders"),
  createFolder: (name: string) => invoke<Folder>("create_folder", { name }),
  renameFolder: (id: Uuid, name: string) =>
    invoke<Folder>("rename_folder", { id, name }),
  deleteFolder: (id: Uuid) => invoke<void>("delete_folder", { id }),
  moveServer: (serverId: Uuid, folderId: Uuid | null) =>
    invoke<Server>("move_server", { serverId, folderId }),

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
    assignToServer: boolean,
    sudoPassword: string | null = null
  ) =>
    invoke<DeployReport>("deploy_key", {
      serverId,
      keyId,
      password,
      sudoPassword,
      assignToServer,
    }),

  checkHardenReadiness: (serverId: Uuid) =>
    invoke<HardenReadiness>("check_harden_readiness", { serverId }),

  hardenServer: (serverId: Uuid, sudoPassword: string | null = null) =>
    invoke<HardenReport>("harden_server", { serverId, sudoPassword }),

  verifyServerHardening: (serverId: Uuid, sudoPassword: string | null = null) =>
    invoke<VerifyHardenReport>("verify_server_hardening", {
      serverId,
      sudoPassword,
    }),

  syncSshConfig: () => invoke<string>("sync_ssh_config"),
};
