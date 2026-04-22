# terminalox

A small cross-platform SSH terminal with **one-click server hardening**.

The use case this was built for: you SSH into a lot of boxes every day, but setting up SSH keys per server is enough friction that you end up typing passwords. terminalox fixes that — add a server once, click **Deployen + Härten**, and the app:

1. generates an Ed25519 keypair,
2. pushes the public key into `~/.ssh/authorized_keys` on the server,
3. verifies key-auth actually works,
4. disables password-login server-side via a `/etc/ssh/sshd_config.d/99-terminalox.conf` drop-in,
5. validates `sshd -t` + reloads sshd,
6. confirms with `sshd -T` that password-auth is really off.

Only if **all six steps** succeed is the server marked 🛡 *hardened*. Any failure triggers an automatic rollback. Deleting the server in the app reverses all of it (removes the drop-in, filters the key out of `authorized_keys`, deletes local key files).

## Features

- Interactive SSH terminal (xterm.js, full PTY).
- Ed25519 key generation + import of existing keys.
- **One-click harden / unharden** (drop-in based, always reversible).
- Key management page with fingerprint, copy-public-to-clipboard, reverse-lookup ("which servers use this key").
- Managed block in `~/.ssh/config` so the system `ssh` command works alongside the app.
- Encrypted backup/restore (AES-256-GCM + Argon2id) for moving state between machines.
- Pure Rust SSH stack (russh) — no OpenSSL dependency.
- Minimal Tauri 2 capabilities: only `core`, `clipboard-manager:write-text`, `dialog:open/save`.

## Download

Pre-built binaries are produced by the GitHub Actions workflow on every tag (`v*`). Grab the matching installer from the [Releases](../../releases) page:

- **macOS:** `terminalox_<version>_aarch64.dmg` (Apple Silicon) or `terminalox_<version>_x64.dmg` (Intel)
- **Windows:** `terminalox_<version>_x64-setup.exe` (NSIS) or `.msi`
- **Linux:** `terminalox_<version>_amd64.AppImage` / `.deb`

### First launch: unsigned app warnings

These builds are **not code-signed** yet (that costs money and an Apple/Microsoft account).

- **macOS:** double-click will say "can't be opened because Apple cannot check it for malicious software". Workaround: right-click the `.app` → **Open** → confirm. Or once: `xattr -cr /Applications/terminalox.app`.
- **Windows:** SmartScreen shows "Windows protected your PC". Click **More info** → **Run anyway**.

## Build from source

### Prerequisites

- [Rust](https://rustup.rs) ≥ 1.85
- [Node.js](https://nodejs.org) ≥ 20
- [pnpm](https://pnpm.io): `npm install -g pnpm`
- Platform-specific:
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`)
  - **Windows:** [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) + [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (bundled with Windows 11)
  - **Linux:** `libwebkit2gtk-4.1-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev build-essential`

### Steps

```bash
git clone https://github.com/<user>/terminalox.git
cd terminalox
pnpm install
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

## Security notes

- Passwords are **never persisted** — used once for initial key deployment and for `sudo` during harden, then discarded.
- Private keys live as files under `~/.ssh/terminalox_<name>` with `0600` permissions. They are not encrypted at rest on the filesystem (the OS user account is the trust boundary).
- Host-key verification is currently accept-any (TOFU + `known_hosts` is on the roadmap — do **not** use this on untrusted networks until that lands).
- sshd-hardening writes to `/etc/ssh/sshd_config.d/99-terminalox.conf` exclusively; your `sshd_config` is never touched. Removing the app undoes the change.
- Backups produced with a passphrase are AES-256-GCM with Argon2id (64 MiB, 3 iterations, 4 lanes). Plain-JSON backups are clearly flagged in the UI with a warning.

## Roadmap

- Host-key verification (known_hosts + TOFU prompt).
- Tabs / multiple simultaneous terminals.
- SFTP panel.
- Port-forwarding.
- Code-signing for macOS/Windows releases.
- Encrypted private keys (passphrase-protected).

## License

MIT — see [LICENSE](LICENSE).
