export type Uuid = string;

export type AuthMode =
  | { mode: "password" }
  | { mode: "key"; key_id: Uuid };

export interface Server {
  id: Uuid;
  name: string;
  host: string;
  port: number;
  user: string;
  auth: AuthMode;
  color?: string | null;
  created_at: string;
  last_used_at?: string | null;
  hardened: boolean;
  hardened_at?: string | null;
  folder_id?: Uuid | null;
}

export interface Folder {
  id: Uuid;
  name: string;
  created_at: string;
}

export interface DeployReport {
  keyDeployed: boolean;
  keyAuthVerified: boolean;
  dropinInstalled: boolean;
  sshdValidated: boolean;
  sshdReloaded: boolean;
  passwordAuthConfirmedDisabled: boolean;
  hardened: boolean;
  serverAuthSwitched: boolean;
  warnings: string[];
}

export interface HardenReport {
  keyAuthVerified: boolean;
  dropinInstalled: boolean;
  sshdValidated: boolean;
  sshdReloaded: boolean;
  passwordAuthConfirmedDisabled: boolean;
  hardened: boolean;
  warnings: string[];
}

export interface VerifyHardenReport {
  dropinPresent: boolean;
  dropinContentIntact: boolean;
  /** true = sshd -T bestätigt PasswordAuthentication=no.
   *  false = effektive Konfig erlaubt Passwort-Login (Drift!).
   *  null = effektiver Check konnte nicht laufen (kein sudo). */
  effectivePasswordAuthOff: boolean | null;
  effectiveCheckError: string | null;
  isHardened: boolean;
  stateChanged: boolean;
  previousState: boolean;
}

export type PrivilegeMethod = "root" | "sudo_nopasswd" | "sudo_with_password";

export type HardenReadiness =
  | { status: "ready"; method: PrivilegeMethod }
  | { status: "unavailable"; reason: string };

export interface NewServerInput {
  name: string;
  host: string;
  port: number;
  user: string;
  color?: string | null;
  auth: AuthMode;
  folder_id?: Uuid | null;
}

export interface KeyInfo {
  id: Uuid;
  name: string;
  algorithm: string;
  public_openssh: string;
  private_path: string;
  public_path: string;
  created_at: string;
  fingerprint?: string | null;
  origin_imported: boolean;
}

export interface HostKeyRejection {
  host: string;
  port: number;
  /** true = known_hosts kennt einen anderen Key gleichen Typs für diesen Host. */
  changed: boolean;
  algorithm: string;
  fingerprint: string;
  keyOpenssh: string;
}

export interface KeyUsage {
  keyId: Uuid;
  servers: Server[];
}

export interface SessionDataEvent {
  sessionId: Uuid;
  dataB64: string;
}

export interface SessionClosedEvent {
  sessionId: Uuid;
  exitCode?: number | null;
  error?: string | null;
}
