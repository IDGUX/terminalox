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

export interface NewServerInput {
  name: string;
  host: string;
  port: number;
  user: string;
  color?: string | null;
  auth: AuthMode;
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
