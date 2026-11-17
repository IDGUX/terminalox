// Demo mode for screenshots and UI work without a Rust backend.
//
// Only active in `vite dev` with `?demo` in the URL; production builds drop
// this module entirely (see the import.meta.env.DEV guard in +page.svelte).
// All data is made up; IPs are from the documentation ranges (RFC 5737).
//
//   ?demo                       servers view
//   ?demo&server=web-01         …with that server selected (opens a terminal)
//   ?demo&view=keys&key=fleet   keys view with a key selected
//   ?demo&dialog=deploy|harden|rotate|import   open a dialog for `server`
//   ?demo&lang=de               force the UI language

import { mockIPC } from "@tauri-apps/api/mocks";
import type { Channel } from "@tauri-apps/api/core";
import { i18n } from "./i18n/index.svelte";
import type { Folder, KeyInfo, Server } from "./types";

const now = Date.now();
const ago = (min: number) => new Date(now - min * 60_000).toISOString();

const folders: Folder[] = [
  { id: "f-prod", name: "Production", created_at: ago(90_000) },
  { id: "f-stage", name: "Staging", created_at: ago(90_000) },
  { id: "f-home", name: "Homelab", created_at: ago(90_000) },
];

const keys: KeyInfo[] = [
  {
    id: "k-fleet",
    name: "fleet_2026",
    algorithm: "ed25519",
    public_openssh:
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHq3vXyDemoKeyOnlyForScreenshots0bW9ja2tleQ terminalox:fleet_2026",
    private_path: "~/.ssh/terminalox_fleet_2026",
    public_path: "~/.ssh/terminalox_fleet_2026.pub",
    created_at: ago(40_000),
    fingerprint: "SHA256:q2Xh0Demo7mN4vR1pLs9KcT8yWbZ3eFgA6uJiO5dEmo",
    origin_imported: false,
    encrypted: false,
  },
  {
    id: "k-laptop",
    name: "laptop",
    algorithm: "ed25519",
    public_openssh:
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDemoLaptopKeyForScreenshotsOnlyAAAAAAAAAA alex@laptop",
    private_path: "~/.ssh/terminalox_imported_laptop",
    public_path: "~/.ssh/terminalox_imported_laptop.pub",
    created_at: ago(200_000),
    fingerprint: "SHA256:Lp7DemoT0kAbC9xYz2Wv4Uq6Rs8Mn1Op3Kl5Jh7Gf9E",
    origin_imported: true,
    encrypted: true,
  },
];

function server(
  id: string,
  name: string,
  user: string,
  host: string,
  folder: string | null,
  opts: Partial<Server> = {},
): Server {
  return {
    id,
    name,
    host,
    port: 22,
    user,
    auth: { mode: "key", key_id: "k-fleet" },
    color: null,
    created_at: ago(80_000),
    last_used_at: null,
    hardened: true,
    hardened_at: ago(30_000),
    folder_id: folder,
    jump_server_id: null,
    ...opts,
  };
}

const servers: Server[] = [
  server("s-web1", "web-01", "deploy", "203.0.113.10", "f-prod", {
    color: "#22c55e",
    last_used_at: ago(3),
  }),
  server("s-web2", "web-02", "deploy", "203.0.113.11", "f-prod", { last_used_at: ago(50) }),
  server("s-bastion", "bastion", "ops", "198.51.100.7", "f-prod", {
    color: "#3b82f6",
    last_used_at: ago(240),
  }),
  server("s-db1", "db-01", "postgres", "10.0.0.5", "f-prod", {
    jump_server_id: "s-bastion",
    last_used_at: ago(900),
  }),
  server("s-stage", "staging-api", "ubuntu", "203.0.113.40", "f-stage", {
    hardened: false,
    hardened_at: null,
    last_used_at: ago(2_000),
  }),
  server("s-preview", "preview", "root", "203.0.113.41", "f-stage", {
    auth: { mode: "password" },
    hardened: false,
    hardened_at: null,
    color: "#eab308",
  }),
  server("s-nas", "nas", "admin", "192.0.2.20", "f-home", {
    auth: { mode: "agent" },
    hardened: false,
    hardened_at: null,
  }),
  server("s-pi", "pi-hole", "pi", "192.0.2.53", "f-home", { last_used_at: ago(6_000) }),
];

const e = "\x1b[";
const green = (s: string) => `${e}1;32m${s}${e}0m`;
const blue = (s: string) => `${e}1;34m${s}${e}0m`;
const dim = (s: string) => `${e}2m${s}${e}0m`;

function shellSession(s: Server): string {
  const prompt = `${green(`${s.user}@${s.name}`)}:${blue("~")}$ `;
  return [
    "Welcome to Ubuntu 24.04.1 LTS (GNU/Linux 6.8.0-45-generic x86_64)",
    "",
    `  System load:  0.08               Processes:             118`,
    `  Usage of /:   31.4% of 76.45GB   Users logged in:       0`,
    `  Memory usage: 23%                IPv4 address for eth0: ${s.host}`,
    "",
    dim("Last login: Tue Nov 17 09:12:44 2026 from 198.51.100.23"),
    `${prompt}sudo sshd -T | grep -E 'passwordauthentication|kbdinteractive'`,
    `passwordauthentication ${s.hardened ? "no" : "yes"}`,
    `kbdinteractiveauthentication ${s.hardened ? "no" : "yes"}`,
    `${prompt}systemctl status nginx --no-pager | head -3`,
    `${green("●")} nginx.service - A high performance web server and a reverse proxy server`,
    "     Loaded: loaded (/usr/lib/systemd/system/nginx.service; enabled; preset: enabled)",
    `     Active: ${green("active (running)")} since Mon 2026-11-16 22:41:07 UTC; 10h ago`,
    `${prompt}uptime`,
    " 09:14:02 up 41 days,  3:17,  1 user,  load average: 0.08, 0.05, 0.01",
    prompt,
  ].join("\r\n");
}

export interface DemoState {
  view: "servers" | "keys";
  serverId: string | null;
  keyId: string | null;
  dialog: string | null;
}

/** Installs the fake backend and returns the requested initial UI state. */
export function setupDemo(params: URLSearchParams): DemoState {
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "list_servers":
        return servers;
      case "list_keys":
        return keys;
      case "list_folders":
        return folders;
      case "servers_using_key":
        return {
          keyId: a.keyId,
          servers: servers.filter((s) => s.auth.mode === "key" && s.auth.key_id === a.keyId),
        };
      case "check_harden_readiness":
        return { status: "ready", method: "sudo_with_password" };
      case "secret_status":
        return { remembered: true };
      case "preview_ssh_config_import":
        return [
          { alias: "monitoring", host: "203.0.113.70", user: "ops", port: 22, identityFile: "~/.ssh/id_ed25519", proxyJump: null, effectiveUser: "ops", alreadyExists: false, problem: null, auth: "key" },
          { alias: "ci-runner", host: "203.0.113.71", user: "ci", port: 2222, identityFile: "~/.ssh/id_ed25519", proxyJump: "bastion", effectiveUser: "ci", alreadyExists: false, problem: null, auth: "key" },
          { alias: "legacy-ftp", host: "192.0.2.80", user: null, port: 22, identityFile: null, proxyJump: null, effectiveUser: "alex", alreadyExists: false, problem: null, auth: "password" },
          { alias: "web-01", host: "203.0.113.10", user: "deploy", port: 22, identityFile: null, proxyJump: null, effectiveUser: "deploy", alreadyExists: true, problem: null, auth: "key" },
        ];
      case "open_session": {
        const s = servers.find((x) => x.id === a.serverId)!;
        const data = a.onData as Channel<ArrayBuffer>;
        setTimeout(() => {
          const bytes = new TextEncoder().encode(shellSession(s));
          data.onmessage(bytes.buffer as ArrayBuffer);
        }, 50);
        return "demo-session";
      }
      default:
        return null;
    }
  });

  const lang = params.get("lang");
  if (lang === "en" || lang === "de") i18n.setLocale(lang);

  const byName = (n: string | null) => servers.find((s) => s.name === n)?.id ?? null;
  const keyByName = (n: string | null) => keys.find((k) => k.name.startsWith(n ?? "\0"))?.id ?? null;
  return {
    view: params.get("view") === "keys" ? "keys" : "servers",
    serverId: byName(params.get("server")),
    keyId: keyByName(params.get("key")),
    dialog: params.get("dialog"),
  };
}
