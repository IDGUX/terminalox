# terminalox

[![CI](https://github.com/IDGUX/terminalox/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/IDGUX/terminalox/actions/workflows/ci.yml)

> **Disclaimer.** terminalox is an experimental open-source side project provided
> **AS-IS** and is **not developed in the course of a commercial activity**. It is
> **not intended for production use**. Installers published under Releases are
> **unsigned**. Use entirely at your own risk; **no warranty**,
> **no fitness-for-purpose claim**, and **no liability** is accepted to the extent
> permitted by law. See [LICENSE](LICENSE) for the full MIT terms.

A small cross-platform SSH terminal with **one-click server hardening**.

The use case this was built for: you SSH into a lot of boxes every day, but setting up SSH keys per server is enough friction that you end up typing passwords. terminalox fixes that — add a server once, click **Deployen + Härten**, and the app:

1. generates an Ed25519 keypair,
2. pushes the public key into `~/.ssh/authorized_keys` on the server,
3. verifies key-auth actually works,
4. installs `/etc/ssh/sshd_config.d/00-terminalox.conf` to disable password and keyboard-interactive authentication,
5. validates `sshd -t` + reloads sshd,
6. opens a fresh SSH connection, verifies key login again and confirms that both password methods are disabled for that connection.

Hardening requires a reloadable systemd SSH service and permission to schedule a system timer. Before editing the SSH configuration, terminalox saves the affected files and schedules a server-side rollback after two minutes. It cancels the rollback and marks the server 🛡 *hardened* after the fresh connection passes verification. If the client disconnects or verification fails, the timer restores the previous configuration and reloads SSH.

## Remove a server entry

- **Server already decommissioned** — Uncheck **SSH-Key auch vom Server entfernen** in the delete dialog. To also remove the associated key files from your computer, select **Zugehörigen privaten SSH-Key lokal löschen**. This works without an SSH connection. If another server entry uses the key, terminalox keeps it.
- **Server still reachable** — Keep **SSH-Key auch vom Server entfernen** selected to remove the public key from the server. If the entry is marked as hardened, terminalox first attempts to undo its SSH configuration changes. It requires a successful password login over a fresh SSH connection before removing the public key. If remote cleanup fails, you can retry or choose **Nur lokal löschen** to remove the entry and keep the local key.

You can remove unused keys from previously deleted server entries in the key management view.

## Features

- Interactive SSH terminal with tabs and split panes (xterm.js, full PTY).
- SSH host-key verification using `~/.ssh/known_hosts`, with a fingerprint prompt for new keys.
- Ed25519 key generation + import of existing keys.
- **One-click SSH hardening** and optional removal of the managed configuration during server cleanup.
- Key management page with fingerprint, copy-public-to-clipboard, reverse-lookup ("which servers use this key").
- Managed block in `~/.ssh/config` so the system `ssh` command works alongside the app.
- Encrypted backup/restore (AES-256-GCM + Argon2id) for moving state between machines.
- Pure Rust SSH stack (russh) — no OpenSSL dependency.
- Minimal Tauri 2 capabilities: only `core`, `clipboard-manager:write-text`, `dialog:open/save`.

## Download

The GitHub Actions release workflow builds installers for `v*` tags and supports manual runs. Grab the matching installer from the [Releases](https://github.com/IDGUX/terminalox/releases) page:

- **macOS:** `terminalox_<version>_aarch64.dmg` (Apple Silicon) or `terminalox_<version>_x64.dmg` (Intel)
- **Windows:** `terminalox_<version>_x64-setup.exe` (NSIS) or `.msi`
- **Linux:** `terminalox_<version>_amd64.AppImage` / `.deb`

### First launch: unsigned app warnings

These builds are **not code-signed** yet (that costs money and an Apple/Microsoft account).

- **macOS:** double-click will say "can't be opened because Apple cannot check it for malicious software". Workaround: right-click the `.app` → **Open** → confirm. Or once: `xattr -cr /Applications/terminalox.app`.
- **Windows:** SmartScreen shows "Windows protected your PC". Click **More info** → **Run anyway**.

## Build from source

### Prerequisites

- Stable [Rust](https://rustup.rs)
- A supported [Node.js](https://nodejs.org) LTS release
- [pnpm](https://pnpm.io) 10 to match CI
- Platform-specific:
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`)
  - **Windows:** [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) + [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (bundled with Windows 11)
  - **Linux:** `libwebkit2gtk-4.1-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev build-essential`

### Steps

```bash
git clone https://github.com/IDGUX/terminalox.git
cd terminalox
pnpm install --frozen-lockfile
pnpm tauri dev          # development, hot-reload
pnpm tauri build        # release build → src-tauri/target/release/bundle/
```

### Universal macOS binary (Apple Silicon + Intel)

```bash
rustup target add x86_64-apple-darwin
pnpm tauri build --target universal-apple-darwin
```

## Project layout

```
src/                    Svelte 5 frontend (SvelteKit + adapter-static, SPA mode)
  lib/
    api.ts              Typed IPC wrappers
    stores/             Reactive app + toast stores
    components/         All UI components
  routes/+page.svelte   Main layout
src-tauri/
  src/
    lib.rs              Tauri builder + command registration
    commands/           All Tauri commands (servers, keys, session, harden, backup)
    ssh/                russh wrappers (client, deploy, revoke, harden, session)
    ssh_config.rs       ~/.ssh/config managed-block writer
    keygen.rs           Ed25519 generation / import
    models.rs           Server, KeyInfo, AuthMode
    storage.rs          JSON persistence in app data dir
  capabilities/         Tauri capability lockdown
  tauri.conf.json       App metadata
```

## Development checks

Run `pnpm check` for frontend diagnostics, `pnpm build` for the static frontend build, and `cargo check --locked --all-targets` from `src-tauri/` to check the Rust targets. GitHub Actions also tests key revocation and timed rollback against OpenSSH on Linux.

## Security notes

- Passwords are **never persisted**. terminalox uses them for SSH login during deployment and cleanup, or for `sudo` when changing the server configuration.
- Managed private keys are unencrypted files under `~/.ssh/terminalox_*`. On Unix, terminalox applies `0600` after writing the file. On Windows, the files inherit filesystem permissions. Passphrase-protected private-key import is currently unsupported.
- terminalox checks `~/.ssh/known_hosts` before sending credentials. For a new key, compare the displayed SHA-256 fingerprint with a fingerprint obtained through a trusted channel before accepting it. A changed key of the same algorithm triggers a warning. Approving a replacement appends it to `known_hosts`; retire the old key separately once you have verified the rotation. An unreadable trust store can be treated as empty, and the parser does not honor OpenSSH `@revoked` markers.
- Host verification uses public host keys. OpenSSH host certificates and certificate authority entries are currently unsupported.
- SSH hardening installs `/etc/ssh/sshd_config.d/00-terminalox.conf` and may add a managed `Include` block to `/etc/ssh/sshd_config`. Uninstalling terminalox leaves the remote configuration in place.
- Hardening changes a daemon-wide SSH configuration. Verification evaluates `Match` rules for the tested SSH user and connection. Other users or source addresses can have different settings. Keep a separate recovery channel available. The transient systemd timer survives an SSH disconnect; a server reboot or a failed service reload may require manual recovery. Pending rollback files are stored in `/etc/ssh/.terminalox-pending`.
- Backups protected by a passphrase use AES-256-GCM with Argon2id (64 MiB, 3 iterations, 4 lanes). Plain JSON backups contain private keys in base64. Set a passphrase to protect exported keys.

## Roadmap

- SFTP panel.
- Port-forwarding.
- Code-signing for macOS/Windows releases.
- Encrypted private keys (passphrase-protected).

## License

MIT — see [LICENSE](LICENSE).
