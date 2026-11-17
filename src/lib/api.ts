import { Channel, invoke as rawInvoke } from "@tauri-apps/api/core";
import { hostKeyPrompt } from "./stores/hostkey.svelte";
import { secretPrompt } from "./stores/secret.svelte";
import { errorText, t } from "./i18n/index.svelte";
import type {
  AuthMode,
  ConfigImportReport,
  DeployReport,
  Folder,
  HardenReadiness,
  HardenReport,
  HostKeyRejection,
  ImportCandidate,
  KeyInfo,
  KeyUsage,
  NewServerInput,
  RotateReport,
  SecretRequest,
  Server,
  SessionClosedEvent,
  Uuid,
  VerifyHardenReport,
} from "./types";

function hostKeyError(e: unknown): HostKeyRejection | null {
  if (e && typeof e === "object" && "hostKey" in e) {
    return (e as { hostKey: HostKeyRejection }).hostKey;
  }
  return null;
}

function secretError(e: unknown): SecretRequest | null {
  if (e && typeof e === "object" && "secretRequired" in e) {
    return (e as { secretRequired: SecretRequest }).secretRequired;
  }
  return null;
}

export { errorText };

/** Wie `invoke`, aber fehlende Zustimmung oder Secrets werden beim Nutzer erfragt
 *  und der Aufruf danach wiederholt:
 *  - unbekannter/geänderter Host-Key → Fingerprint bestätigen (der Server lehnt
 *    vor jeder Authentifizierung ab, die Wiederholung ist also unkritisch),
 *  - fehlendes/falsches Passwort oder Key-Passphrase → Eingabe-Dialog. */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await rawInvoke<T>(cmd, args);
  } catch (e) {
    const hk = hostKeyError(e);
    if (hk) {
      if (!(await hostKeyPrompt.ask(hk))) {
        throw t("api.hostKeyAborted", { host: hk.host });
      }
      return invoke<T>(cmd, args);
    }
    const sr = secretError(e);
    if (sr) {
      if (!(await secretPrompt.ask(sr))) {
        throw sr.kind === "passphrase"
          ? t("api.passphraseCancelled", { label: sr.label })
          : t("api.passwordCancelled", { label: sr.label });
      }
      return invoke<T>(cmd, args);
    }
    throw errorText(e);
  }
}

export const api = {
  listServers: () => invoke<Server[]>("list_servers"),
  addServer: (input: NewServerInput) =>
    invoke<Server>("add_server", { input }),
  deleteServer: (id: Uuid) => invoke<void>("delete_server", { id }),
  updateServer: (
    input: { id: Uuid } & Partial<Pick<Server, "name" | "host" | "port" | "user">> & {
      color?: string | null;
      auth?: AuthMode;
      jump_server_id?: Uuid | null;
    },
  ) =>
    invoke<Server>("update_server", { input }),

  listFolders: () => invoke<Folder[]>("list_folders"),
  createFolder: (name: string) => invoke<Folder>("create_folder", { name }),
  renameFolder: (id: Uuid, name: string) =>
    invoke<Folder>("rename_folder", { id, name }),
  deleteFolder: (id: Uuid) => invoke<void>("delete_folder", { id }),
  moveServer: (serverId: Uuid, folderId: Uuid | null) =>
    invoke<Server>("move_server", { serverId, folderId }),

  listKeys: () => invoke<KeyInfo[]>("list_keys"),
  generateKey: (name: string, passphrase: string | null = null, rememberPassphrase = false) =>
    invoke<KeyInfo>("generate_key", { name, passphrase, rememberPassphrase }),
  importKey: (sourcePath: string, name: string) =>
    invoke<KeyInfo>("import_key", { sourcePath, name }),
  serversUsingKey: (keyId: Uuid) =>
    invoke<KeyUsage>("servers_using_key", { keyId }),
  deleteKey: (id: Uuid) => invoke<void>("delete_key", { id }),

  /** Output arrives as raw bytes on `onData`; `onClosed` fires once at the end. */
  openSession: (
    serverId: Uuid,
    cols: number,
    rows: number,
    onData: (bytes: Uint8Array) => void,
    onClosed: (ev: SessionClosedEvent) => void,
  ) => {
    const data = new Channel<ArrayBuffer>();
    data.onmessage = (buf) => onData(new Uint8Array(buf));
    const closed = new Channel<SessionClosedEvent>();
    closed.onmessage = onClosed;
    return invoke<Uuid>("open_session", {
      serverId,
      password: null,
      cols,
      rows,
      onData: data,
      onClosed: closed,
    });
  },
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
  previewSshConfigImport: () => invoke<ImportCandidate[]>("preview_ssh_config_import"),
  importSshConfigHosts: (aliases: string[]) =>
    invoke<ConfigImportReport>("import_ssh_config_hosts", { aliases }),

  rotateKey: (serverId: Uuid) => invoke<RotateReport>("rotate_key", { serverId }),

  secretStatus: (kind: SecretRequest["kind"], id: Uuid) =>
    invoke<{ remembered: boolean }>("secret_status", { kind, id }),
  forgetSecret: (kind: SecretRequest["kind"], id: Uuid) =>
    invoke<void>("forget_secret", { kind, id }),
};
