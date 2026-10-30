use base64::prelude::*;
use russh::ChannelMsg;

use crate::error::{AppError, AppResult};
use crate::ssh::client::{self, KnownHostsVerify};

// Prefix `00-` so the drop-in is loaded before other files in
// /etc/ssh/sshd_config.d/ (e.g. Ubuntu's 50-cloud-init.conf which sets
// `PasswordAuthentication yes`). sshd applies the *first* directive it
// sees per keyword, so our file must be alphabetically first to win.
pub const DROPIN_PATH: &str = "/etc/ssh/sshd_config.d/00-terminalox.conf";
// Legacy path from earlier builds — still cleaned up on unharden for
// forward compatibility with servers hardened by older app versions.
const LEGACY_DROPIN_PATH: &str = "/etc/ssh/sshd_config.d/99-terminalox.conf";
const DROPIN_CONTENT: &str = "# Managed by terminalox — do not edit.\n\
                              # Removed automatically when the server is deleted from the app.\n\
                              PasswordAuthentication no\n\
                              KbdInteractiveAuthentication no\n";

/// How the remote session can elevate to root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivilegeMethod {
    /// Logged-in user is uid 0 — no escalation needed.
    Root,
    /// `sudo -n` works (NOPASSWD sudoers or cached credential).
    SudoNopasswd,
    /// User is allowed to sudo but `sudo -n` required a password.
    /// Caller must supply a sudo password when actually hardening.
    SudoWithPassword,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HardenReadiness {
    /// Harden can proceed; if `method == SudoWithPassword`, the caller
    /// must collect a sudo password from the user.
    Ready { method: PrivilegeMethod },
    /// Harden is not possible on this server/user combination.
    Unavailable { reason: String },
}

#[derive(Default, Debug, Clone)]
pub struct HardenOutcome {
    pub dropin_installed: bool,
    pub sshd_validated: bool,
    pub sshd_reloaded: bool,
    pub password_auth_confirmed_disabled: bool,
    pub stderr: String,
}

impl HardenOutcome {
    pub fn all_ok(&self) -> bool {
        self.dropin_installed
            && self.sshd_validated
            && self.sshd_reloaded
            && self.password_auth_confirmed_disabled
    }
}

/// Probe the remote host to decide how hardening can be performed.
/// Captures `id -u`, `sudo -n true` exit+stderr, and whether the sudo binary
/// exists — enough to distinguish root / NOPASSWD / needs-password / not-allowed.
/// No password is ever sent by this probe.
pub async fn probe_readiness(
    session: russh::client::Handle<KnownHostsVerify>,
) -> AppResult<HardenReadiness> {
    // Emit structured lines; capture sudo -n's stderr into a base64-encoded line
    // so we can inspect it without worrying about newlines / escaping.
    let script = r#"echo "UID:$(id -u)"
if ! command -v sudo >/dev/null 2>&1; then
  echo "SUDO:missing"
else
  err=$(sudo -n true 2>&1 1>/dev/null)
  rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "SUDO:nopasswd"
  else
    echo "SUDO:denied:$rc"
    echo "SUDO_ERR:$(printf '%s' "$err" | base64 | tr -d '\n')"
  fi
fi
"#;
    let (_exit, stdout, _stderr) = exec_collect(session, script, "").await?;

    let mut uid: Option<u32> = None;
    let mut sudo: Option<String> = None;
    let mut sudo_err_b64: Option<String> = None;
    for line in stdout.lines() {
        if let Some(v) = line.strip_prefix("UID:") {
            uid = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("SUDO:") {
            sudo = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("SUDO_ERR:") {
            sudo_err_b64 = Some(v.trim().to_string());
        }
    }

    if uid == Some(0) {
        return Ok(HardenReadiness::Ready {
            method: PrivilegeMethod::Root,
        });
    }

    let sudo_err = sudo_err_b64
        .and_then(|b| BASE64_STANDARD.decode(b).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    let err_lc = sudo_err.to_lowercase();

    match sudo.as_deref() {
        Some("nopasswd") => Ok(HardenReadiness::Ready {
            method: PrivilegeMethod::SudoNopasswd,
        }),
        Some(s) if s.starts_with("denied") => {
            // Heuristic on sudo's error text:
            // - "password is required" / "a password is required" → user IS allowed, just needs pw
            // - "not in the sudoers" / "may not run sudo" → user is NOT allowed
            if err_lc.contains("password is required") || err_lc.contains("password required") {
                Ok(HardenReadiness::Ready {
                    method: PrivilegeMethod::SudoWithPassword,
                })
            } else if err_lc.contains("not in the sudoers")
                || err_lc.contains("may not run sudo")
                || err_lc.contains("not allowed")
            {
                Ok(HardenReadiness::Unavailable {
                    reason: format!(
                        "Dein User ist nicht in der sudoers-Liste und kann daher kein sudo \
                         ausführen. Härten ist so nicht möglich. Optionen: als root-User \
                         einloggen, oder den User auf dem Server zur sudo-Gruppe hinzufügen \
                         (`usermod -aG sudo <user>`). Server-Antwort: {}",
                        sudo_err.trim()
                    ),
                })
            } else {
                // Unknown sudo error — assume password is needed, user can try.
                // If they enter one and it still fails, the harden script will roll back.
                Ok(HardenReadiness::Ready {
                    method: PrivilegeMethod::SudoWithPassword,
                })
            }
        }
        Some("missing") => Ok(HardenReadiness::Unavailable {
            reason: "Auf dem Server ist kein sudo installiert und der User ist nicht root. \
                     Härten ist nicht möglich."
                .into(),
        }),
        _ => Ok(HardenReadiness::Unavailable {
            reason: format!("Unerwartete Antwort vom Server: {stdout}"),
        }),
    }
}

/// Run the hardening flow over an authenticated session. The password (if any)
/// is embedded base64-encoded into the script itself — the script then
/// primes sudo's credential cache via an internal heredoc and uses `sudo -n`
/// for the rest. Everything happens in a single channel = single TTY, so the
/// credential cache stays valid throughout. Self-rolls-back on any failure.
pub async fn harden(
    session: russh::client::Handle<KnownHostsVerify>,
    sudo_password: Option<&str>,
) -> AppResult<HardenOutcome> {
    let script = build_harden_script(sudo_password);
    let (exit, stdout, stderr) = exec_collect(session, &script, "").await?;

    let outcome = parse_harden_stdout(&stdout, stderr.clone());

    if exit != 0 && !outcome.all_ok() {
        return Ok(HardenOutcome {
            stderr: format!("exit {exit}: {stderr}"),
            ..outcome
        });
    }
    Ok(outcome)
}

/// Re-check the live hardening state on the remote.
/// Read-only; never writes to the server. Combines:
///   1. drop-in file present at the expected path
///   2. drop-in content still contains `PasswordAuthentication no`
///   3. (when sudo is available) `sshd -T` confirms effective `PasswordAuthentication no`
/// The third check is the ground truth; the first two are cheap and
/// don't require privileges, so we always do them and surface them
/// separately for clear diagnostics.
pub async fn verify(
    session: russh::client::Handle<KnownHostsVerify>,
    sudo_password: Option<&str>,
) -> AppResult<VerifyOutcome> {
    let script = build_verify_script(sudo_password);
    let (_exit, stdout, _stderr) = exec_collect(session, &script, "").await?;
    Ok(parse_verify_stdout(&stdout))
}

#[derive(Default, Debug, Clone)]
pub struct VerifyOutcome {
    pub dropin_present: bool,
    pub dropin_content_intact: bool,
    /// `Some(true)` = effective PasswordAuthentication is `no` (server is locked down).
    /// `Some(false)` = effective PasswordAuthentication is `yes` (drift!).
    /// `None` = couldn't determine (no sudo / sshd -T unavailable).
    pub effective_password_auth_off: Option<bool>,
    pub effective_check_error: Option<String>,
}

impl VerifyOutcome {
    /// Conservative verdict: prefer the effective check when present, otherwise
    /// trust the drop-in file. Either signal showing "not hardened" wins.
    pub fn is_hardened(&self) -> bool {
        if self.effective_password_auth_off == Some(false) {
            return false;
        }
        self.dropin_present
            && self.dropin_content_intact
            && self.effective_password_auth_off != Some(false)
    }
}

pub async fn unharden(
    session: russh::client::Handle<KnownHostsVerify>,
    sudo_password: Option<&str>,
) -> AppResult<()> {
    let script = build_unharden_script(sudo_password);
    let (exit, _stdout, stderr) = exec_collect(session, &script, "").await?;
    if exit != 0 {
        return Err(AppError::RemoteCommandFailed(format!(
            "unharden exit {exit}: {stderr}"
        )));
    }
    Ok(())
}

/// Emit a shell prelude that sets `$SUDO` for the rest of the script.
/// The prelude may inline a (base64-encoded) sudo password — if so, it
/// primes sudo's credential cache in-script via an internal heredoc.
/// This keeps everything in one TTY/channel, avoiding tty_tickets cache
/// invalidation between channels.
/// Exits: 20 = no privilege path, 21 = password was needed but wrong/missing.
fn privilege_prelude(sudo_password: Option<&str>) -> String {
    // Base64 empty string if no password — the script then treats it as absent.
    let pw_b64 = BASE64_STANDARD.encode(sudo_password.unwrap_or(""));
    format!(
        r#"SUDO_PW_B64='{pw_b64}'
if [ "$(id -u)" -eq 0 ]; then
  SUDO=""
elif command -v sudo >/dev/null 2>&1; then
  if sudo -n true 2>/dev/null; then
    SUDO="sudo -n"
  elif [ -n "$SUDO_PW_B64" ]; then
    __SUDO_PW=$(printf '%s' "$SUDO_PW_B64" | base64 -d)
    if ! sudo -S -p '' -v 2>/dev/null <<EOF_PW
$__SUDO_PW
EOF_PW
    then
      unset __SUDO_PW SUDO_PW_B64
      echo 'STEP:sudo_auth_failed' >&2
      exit 21
    fi
    unset __SUDO_PW SUDO_PW_B64
    SUDO="sudo -n"
  else
    echo 'STEP:sudo_password_required' >&2
    exit 21
  fi
else
  echo 'STEP:no_privilege' >&2
  exit 20
fi
"#
    )
}

fn build_harden_script(sudo_password: Option<&str>) -> String {
    let content_b64 = BASE64_STANDARD.encode(DROPIN_CONTENT);
    let prelude = privilege_prelude(sudo_password);
    format!(
        r#"bash -s <<'TERMINALOX_EOF'
set -u
{prelude}
DROPIN='{path}'
STAGE="$DROPIN.terminalox.new"
INCLUDE_WAS_ADDED=0

# --- Reload sshd. Tries systemd, SysV-init, and OpenRC. -------------------
reload_sshd() {{
  for svc in ssh sshd; do
    if $SUDO systemctl reload "$svc" 2>/dev/null; then return 0; fi
  done
  for svc in ssh sshd; do
    if $SUDO service "$svc" reload 2>/dev/null; then return 0; fi
  done
  # OpenRC (Alpine/Gentoo). Reload action may be absent → try restart.
  for svc in sshd ssh; do
    if $SUDO rc-service "$svc" reload 2>/dev/null; then return 0; fi
    if $SUDO rc-service "$svc" restart 2>/dev/null; then return 0; fi
  done
  return 1
}}

# --- Revert the include marker block we may have appended. ---------------
revert_include() {{
  if [ "$INCLUDE_WAS_ADDED" -ne 1 ]; then return 0; fi
  $SUDO awk '/# BEGIN_TERMINALOX_INCLUDE/{{skip=1; next}} /# END_TERMINALOX_INCLUDE/{{skip=0; next}} !skip' /etc/ssh/sshd_config \
    | $SUDO tee /etc/ssh/sshd_config.terminalox.new >/dev/null
  $SUDO chmod 644 /etc/ssh/sshd_config.terminalox.new
  $SUDO mv /etc/ssh/sshd_config.terminalox.new /etc/ssh/sshd_config
}}

# --- Full rollback: drop-in away, include marker away, reload. ------------
rollback() {{
  $SUDO rm -f "$DROPIN"
  revert_include
  reload_sshd >/dev/null 2>&1 || true
}}

# Step 0: ensure the drop-in directory is honored by sshd.
# Modern distros (Debian 11+, Ubuntu 20.04+, Fedora, RHEL/Rocky/Alma 8+)
# include /etc/ssh/sshd_config.d/*.conf out of the box. Older or custom
# sshd_config files may lack the Include directive — without it, our
# drop-in is silently ignored.
if ! $SUDO grep -qE '^[[:space:]]*Include[[:space:]]+.*sshd_config\.d' /etc/ssh/sshd_config 2>/dev/null; then
  if ! $SUDO grep -q '# BEGIN_TERMINALOX_INCLUDE' /etc/ssh/sshd_config 2>/dev/null; then
    if $SUDO tee -a /etc/ssh/sshd_config >/dev/null <<EOF_INC

# BEGIN_TERMINALOX_INCLUDE
# Added by terminalox so /etc/ssh/sshd_config.d/*.conf is honored.
# Removed automatically when the server is unhardened.
Include /etc/ssh/sshd_config.d/*.conf
# END_TERMINALOX_INCLUDE
EOF_INC
    then
      INCLUDE_WAS_ADDED=1
      echo 'STEP:include_added'
    fi
  fi
fi

$SUDO mkdir -p /etc/ssh/sshd_config.d
echo '{content}' | base64 -d | $SUDO tee "$STAGE" >/dev/null
$SUDO chmod 644 "$STAGE"
# Activate drop-in
$SUDO mv "$STAGE" "$DROPIN"
echo 'STEP:dropin_installed'

# Validate full sshd config (drop-in + maybe new include).
if ! $SUDO sshd -t 2>&1; then
  rollback
  echo 'STEP:validation_failed' >&2
  exit 2
fi
echo 'STEP:sshd_validated'

if ! reload_sshd; then
  rollback
  echo 'STEP:reload_failed' >&2
  exit 3
fi
echo 'STEP:sshd_reloaded'

# Verify effective config — ground truth.
if $SUDO sshd -T 2>/dev/null | grep -qiE '^passwordauthentication no'; then
  echo 'STEP:password_auth_off'
else
  rollback
  echo 'STEP:password_auth_verify_failed' >&2
  exit 4
fi
TERMINALOX_EOF"#,
        prelude = prelude,
        path = DROPIN_PATH,
        content = content_b64
    )
}

fn build_unharden_script(sudo_password: Option<&str>) -> String {
    let prelude = privilege_prelude(sudo_password);
    format!(
        r#"bash -s <<'TERMINALOX_EOF'
set -u
{prelude}

# Remove our drop-in files (current + legacy path).
for f in '{path}' '{legacy}'; do
  if [ -f "$f" ]; then $SUDO rm -f "$f"; fi
done

# If we previously added an Include directive, remove the marker block.
if $SUDO grep -q '# BEGIN_TERMINALOX_INCLUDE' /etc/ssh/sshd_config 2>/dev/null; then
  $SUDO awk '/# BEGIN_TERMINALOX_INCLUDE/{{skip=1; next}} /# END_TERMINALOX_INCLUDE/{{skip=0; next}} !skip' /etc/ssh/sshd_config \
    | $SUDO tee /etc/ssh/sshd_config.terminalox.new >/dev/null
  $SUDO chmod 644 /etc/ssh/sshd_config.terminalox.new
  $SUDO mv /etc/ssh/sshd_config.terminalox.new /etc/ssh/sshd_config
fi

# Reload sshd via whatever init system is available.
for svc in ssh sshd; do $SUDO systemctl reload "$svc" 2>/dev/null && exit 0; done
for svc in ssh sshd; do $SUDO service "$svc" reload 2>/dev/null && exit 0; done
for svc in sshd ssh; do
  $SUDO rc-service "$svc" reload 2>/dev/null && exit 0
  $SUDO rc-service "$svc" restart 2>/dev/null && exit 0
done
# If no reload worked, still report success — drop-ins are gone, new sshd
# behavior will apply on the next config read.
exit 0
TERMINALOX_EOF"#,
        prelude = prelude,
        path = DROPIN_PATH,
        legacy = LEGACY_DROPIN_PATH
    )
}

/// Verify script. Stays read-only and survives missing sudo:
///   - File check via `test -f` + content grep run unprivileged
///   - `sshd -T` is best-effort: if no privilege path, we just skip it
fn build_verify_script(sudo_password: Option<&str>) -> String {
    // We DON'T want the prelude to hard-exit if no sudo — verify must keep
    // running even on a non-sudo user. Build a softer variant inline.
    let pw_b64 = BASE64_STANDARD.encode(sudo_password.unwrap_or(""));
    format!(
        r#"bash -s <<'TERMINALOX_EOF'
set -u
DROPIN='{path}'

# 1) Drop-in file
if [ -f "$DROPIN" ]; then
  echo 'DROPIN:present'
  if grep -qiE '^[[:space:]]*PasswordAuthentication[[:space:]]+no[[:space:]]*$' "$DROPIN" 2>/dev/null; then
    echo 'DROPIN_CONTENT:intact'
  else
    echo 'DROPIN_CONTENT:tampered'
  fi
else
  echo 'DROPIN:missing'
fi

# 2) Effective sshd config — best effort, needs root/sudo
SUDO_PW_B64='{pw_b64}'
HAVE_PRIV=0
SUDO_VERIFY=""
if [ "$(id -u)" -eq 0 ]; then
  HAVE_PRIV=1
elif command -v sudo >/dev/null 2>&1; then
  if sudo -n true 2>/dev/null; then
    HAVE_PRIV=1
    SUDO_VERIFY="sudo -n"
  elif [ -n "$SUDO_PW_B64" ]; then
    __SUDO_PW=$(printf '%s' "$SUDO_PW_B64" | base64 -d)
    if sudo -S -p '' -v 2>/dev/null <<EOF_PW
$__SUDO_PW
EOF_PW
    then
      HAVE_PRIV=1
      SUDO_VERIFY="sudo -n"
    else
      echo 'EFFECTIVE:skipped:sudo_auth_failed'
    fi
    unset __SUDO_PW SUDO_PW_B64
  else
    echo 'EFFECTIVE:skipped:needs_sudo_password'
  fi
else
  echo 'EFFECTIVE:skipped:no_sudo'
fi

if [ "$HAVE_PRIV" -eq 1 ]; then
  effective=$($SUDO_VERIFY sshd -T 2>/dev/null | awk 'tolower($1)=="passwordauthentication"{{print tolower($2); exit}}')
  if [ -z "$effective" ]; then
    echo 'EFFECTIVE:unknown'
  elif [ "$effective" = "no" ]; then
    echo 'EFFECTIVE:off'
  else
    echo "EFFECTIVE:on:$effective"
  fi
fi
TERMINALOX_EOF"#,
        path = DROPIN_PATH,
        pw_b64 = pw_b64
    )
}

fn parse_verify_stdout(stdout: &str) -> VerifyOutcome {
    let mut o = VerifyOutcome::default();
    for line in stdout.lines() {
        let line = line.trim();
        match line {
            "DROPIN:present" => o.dropin_present = true,
            "DROPIN:missing" => o.dropin_present = false,
            "DROPIN_CONTENT:intact" => o.dropin_content_intact = true,
            "DROPIN_CONTENT:tampered" => o.dropin_content_intact = false,
            "EFFECTIVE:off" => o.effective_password_auth_off = Some(true),
            s if s.starts_with("EFFECTIVE:on") => o.effective_password_auth_off = Some(false),
            "EFFECTIVE:unknown" => {
                o.effective_check_error =
                    Some("sshd -T lieferte keinen PasswordAuthentication-Wert".into());
            }
            s if s.starts_with("EFFECTIVE:skipped:") => {
                let reason = s.trim_start_matches("EFFECTIVE:skipped:");
                o.effective_check_error = Some(match reason {
                    "no_sudo" => "Kein sudo verfügbar — Effektiv-Check übersprungen".into(),
                    "needs_sudo_password" => {
                        "Effektiv-Check braucht ein sudo-Passwort".into()
                    }
                    "sudo_auth_failed" => "sudo hat das Passwort abgelehnt".into(),
                    other => format!("Effektiv-Check übersprungen: {other}"),
                });
            }
            _ => {}
        }
    }
    o
}

fn parse_harden_stdout(stdout: &str, stderr: String) -> HardenOutcome {
    let mut o = HardenOutcome {
        stderr,
        ..Default::default()
    };
    for line in stdout.lines() {
        match line.trim() {
            "STEP:dropin_installed" => o.dropin_installed = true,
            "STEP:sshd_validated" => o.sshd_validated = true,
            "STEP:sshd_reloaded" => o.sshd_reloaded = true,
            "STEP:password_auth_off" => o.password_auth_confirmed_disabled = true,
            _ => {}
        }
    }
    o
}

async fn exec_collect(
    session: russh::client::Handle<KnownHostsVerify>,
    script: &str,
    stdin_bytes: &str,
) -> AppResult<(u32, String, String)> {
    let mut channel = session.channel_open_session().await?;
    channel.exec(true, script.as_bytes()).await?;
    channel.data(stdin_bytes.as_bytes()).await?;
    channel.eof().await?;

    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let mut exit: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
            ChannelMsg::ExtendedData { ref data, ext } if ext == 1 => {
                stderr.extend_from_slice(data)
            }
            ChannelMsg::ExitStatus { exit_status } => exit = Some(exit_status),
            _ => {}
        }
    }
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;

    Ok((
        exit.unwrap_or(1),
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    ))
}

/// Convenience: open a fresh key-authenticated session to the host. Used both as
/// a pre-flight proof that key auth actually works, and as the transport for the
/// hardening/unhardening scripts.
pub async fn open_key_session(
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
) -> AppResult<russh::client::Handle<KnownHostsVerify>> {
    let mut session = client::connect(host, port).await?;
    client::auth_key(&mut session, user, key_path).await?;
    Ok(session)
}
