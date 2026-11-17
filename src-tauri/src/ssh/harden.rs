use base64::prelude::*;
use russh::ChannelMsg;

use crate::error::{AppError, AppResult};
use crate::ssh::client::{self, KnownHostsVerify};

// Prefix `00-` so the drop-in is loaded before other files in
// /etc/ssh/sshd_config.d/ (e.g. Ubuntu's 50-cloud-init.conf which sets
// `PasswordAuthentication yes`). sshd applies the *first* directive it
// sees per keyword, so our file must be alphabetically first to win.
pub const DROPIN_PATH: &str = "/etc/ssh/sshd_config.d/00-terminalox.conf";
const TRANSACTION: &str = include_str!("harden_transaction.sh");
const ROLLBACK_SECONDS: u64 = 120;
const SSH_STEP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);

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
    committed: bool,
}

impl HardenOutcome {
    pub fn all_ok(&self) -> bool {
        self.committed
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
    // The transaction needs bash, flock and a timer (systemd-run or setsid).
    // BusyBox provides flock and setsid, so Alpine only lacks bash by default.
    let script = r#"missing=
command -v bash >/dev/null 2>&1 || missing="$missing bash"
command -v flock >/dev/null 2>&1 || missing="$missing flock"
if ! command -v systemd-run >/dev/null 2>&1 && ! command -v setsid >/dev/null 2>&1; then
    missing="$missing setsid"
fi
if [ -z "$missing" ]; then
    echo "ROLLBACK:available"
else
    echo "MISSING:$missing"
fi
echo "UID:$(id -u)"
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
    if !stdout.lines().any(|line| line == "ROLLBACK:available") {
        let missing = stdout
            .lines()
            .find_map(|line| line.strip_prefix("MISSING:"))
            .map(str::trim)
            .unwrap_or("bash, flock, setsid");
        return Ok(HardenReadiness::Unavailable {
            reason: format!(
                "Für den automatischen Rollback fehlen auf dem Server: {missing}. \
                 Bitte nachinstallieren (z. B. Alpine: `apk add bash util-linux`, \
                 Debian/Ubuntu: `apt install bash util-linux`) und erneut versuchen."
            ),
        });
    }

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

/// Apply independently of the client, then confirm over a newly authenticated connection.
pub async fn harden(
    session: russh::client::Handle<KnownHostsVerify>,
    target: (&str, u16, &str, &str),
    sudo_password: Option<&str>,
) -> AppResult<HardenOutcome> {
    let token = uuid::Uuid::new_v4();
    let password = BASE64_STANDARD.encode(sudo_password.unwrap_or(""));
    let apply = build_transaction_command("apply", token, ROLLBACK_SECONDS);
    let rollback_notice = "Solange die Änderung unbestätigt ist, bleibt der automatische Rollback aktiv.";
    let (exit, stdout, stderr) =
        match tokio::time::timeout(SSH_STEP_TIMEOUT, exec_collect(session, &apply, &password)).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                return Ok(HardenOutcome {
                    stderr: format!("{error}. {rollback_notice}"),
                    ..Default::default()
                });
            }
            Err(_) => {
                return Ok(HardenOutcome {
                    stderr: format!("SSH-Härtung hat das Zeitlimit überschritten. {rollback_notice}"),
                    ..Default::default()
                });
            }
        };
    let rolled_back = stdout.lines().any(|line| line.trim() == "STEP:rolled_back");
    let mut outcome = parse_harden_stdout(&stdout, stderr);
    if exit != 0 || !outcome.dropin_installed || !outcome.sshd_validated || !outcome.sshd_reloaded {
        let notice = if rolled_back {
            "Die vorherige SSH-Konfiguration wurde sofort wiederhergestellt."
        } else {
            rollback_notice
        };
        outcome.stderr = format!("exit {exit}: {}. {notice}", outcome.stderr.trim());
        return Ok(outcome);
    }

    let (host, port, user, key_path) = target;
    let confirmation = async {
        let fresh = connect_after_reload(host, port, user, key_path).await?;
        let confirm = build_transaction_command("confirm", token, ROLLBACK_SECONDS);
        let (exit, stdout, stderr) = exec_collect(fresh, &confirm, &password).await?;
        if exit != 0 || !stdout.lines().any(|line| line.trim() == "STEP:password_auth_off") {
            return Err(AppError::RemoteCommandFailed(format!("exit {exit}: {stderr}")));
        }
        Ok::<_, AppError>(())
    };
    match tokio::time::timeout(SSH_STEP_TIMEOUT, confirmation).await {
        Ok(Ok(())) => {
            outcome.password_auth_confirmed_disabled = true;
            outcome.committed = true;
        }
        Ok(Err(error)) => {
            outcome.stderr = format!(
                "Bestätigung über neue SSH-Verbindung fehlgeschlagen: {error}. {rollback_notice}"
            );
        }
        Err(_) => {
            outcome.stderr = format!(
                "Bestätigung über neue SSH-Verbindung hat das Zeitlimit überschritten. {rollback_notice}"
            );
        }
    }
    Ok(outcome)
}

async fn connect_after_reload(
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
) -> AppResult<russh::client::Handle<KnownHostsVerify>> {
    use russh::{client::AuthResult, MethodKind};

    let probe = || async {
        let mut session = client::connect(host, port).await?;
        match session.authenticate_none(user).await? {
            AuthResult::Failure { remaining_methods, .. }
                if !remaining_methods.contains(&MethodKind::Password)
                    && !remaining_methods.contains(&MethodKind::KeyboardInteractive) => {}
            _ => {
                return Err(AppError::Other(
                    "Die neue SSH-Verbindung bietet weiterhin Passwortverfahren an oder verlangt keine Authentifizierung.".into(),
                ));
            }
        }
        client::auth_key(&mut session, user, key_path).await?;
        Ok(session)
    };

    // systemctl reports completion when the daemon is signalled; the
    // listener may briefly stop accepting connections while it re-execs.
    for pause in [250, 500, 1000] {
        match probe().await {
            Ok(session) => return Ok(session),
            Err(error @ AppError::HostKey(_)) => return Err(error),
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(pause)).await,
        }
    }
    probe().await
}

/// Inspect both password methods for this session without changing sshd.
pub async fn verify(
    session: russh::client::Handle<KnownHostsVerify>,
    sudo_password: Option<&str>,
) -> AppResult<VerifyOutcome> {
    let file_check = format!(
        r#"if [ -f '{DROPIN_PATH}' ]; then
    printf '%s\n' 'DROPIN:present'
    if grep -qiE '^[[:space:]]*PasswordAuthentication[[:space:]]+no[[:space:]]*$' '{DROPIN_PATH}' &&
       grep -qiE '^[[:space:]]*KbdInteractiveAuthentication[[:space:]]+no[[:space:]]*$' '{DROPIN_PATH}'; then
        printf '%s\n' 'DROPIN_CONTENT:intact'
    fi
fi
"#
    );
    let command = build_transaction_command("verify", uuid::Uuid::nil(), 0);
    let password = BASE64_STANDARD.encode(sudo_password.unwrap_or(""));
    let (exit, stdout, stderr) =
        exec_collect(session, &(file_check + &command), &password).await?;
    let mut outcome = parse_verify_stdout(&stdout);
    if exit != 0 {
        outcome.effective_password_auth_off = None;
        outcome.effective_check_error = Some(stderr.trim().to_string());
    }
    Ok(outcome)
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
    pub fn is_hardened(&self) -> bool {
        self.dropin_present
            && self.dropin_content_intact
            && self.effective_password_auth_off == Some(true)
    }
}

pub async fn unharden(
    session: russh::client::Handle<KnownHostsVerify>,
    sudo_password: Option<&str>,
) -> AppResult<()> {
    let command = build_transaction_command("unharden", uuid::Uuid::nil(), 0);
    let password = BASE64_STANDARD.encode(sudo_password.unwrap_or(""));
    let (exit, _stdout, stderr) = exec_collect(session, &command, &password).await?;
    if exit != 0 {
        return Err(AppError::RemoteCommandFailed(format!(
            "unharden exit {exit}: {stderr}"
        )));
    }
    Ok(())
}

fn build_transaction_command(action: &str, token: uuid::Uuid, delay: u64) -> String {
    build_transaction_command_with_timer(action, token, delay, "auto")
}

/// `timer_mode` is `auto` (systemd timer when available, else setsid) or
/// `detached` (always setsid; lets CI cover hosts without systemd).
fn build_transaction_command_with_timer(
    action: &str,
    token: uuid::Uuid,
    delay: u64,
    timer_mode: &str,
) -> String {
    let script = TRANSACTION.replace('\'', "'\\''");
    format!(
        r#"program='{script}'
set -- -c "$program" terminalox '{action}' '{token}' "${{SSH_CONNECTION:-}}" "$(id -un)" '{delay}' '{timer_mode}'
if [ "$(id -u)" -eq 0 ]; then
    exec /bin/bash "$@"
elif ! command -v sudo >/dev/null; then
    echo 'STEP:no_privilege' >&2
    exit 20
fi
IFS= read -r password || true
if [ -n "$password" ]; then
    {{ printf '%s' "$password" | base64 -d; printf '\n'; }} |
        sudo -k -S -p '' -- /bin/bash "$@" || exit $?
else
    exec sudo -n -- /bin/bash "$@"
fi"#
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const HOST: &str = "127.0.0.1";
    const PORT: u16 = 22222;
    const USER: &str = "terminalox-test";
    const PENDING: &str = "/etc/ssh/.terminalox-pending";

    /// Apply without confirming, drop the transport and wait for the timer to
    /// restore the previous configuration byte for byte.
    async fn assert_timer_rolls_back(key: &str, password: &str, timer_mode: &str) {
        let ssh_config = "/etc/ssh/sshd_config";
        let original_config = std::fs::read(ssh_config).unwrap();
        let original_dropin = std::fs::read(DROPIN_PATH).unwrap();

        let session = open_key_session(HOST, PORT, USER, key).await.unwrap();
        let mut channel = session.channel_open_session().await.unwrap();
        let apply =
            build_transaction_command_with_timer("apply", uuid::Uuid::new_v4(), 10, timer_mode);
        channel.exec(true, apply.as_bytes()).await.unwrap();
        let input = BASE64_STANDARD.encode(password);
        channel.data(input.as_bytes()).await.unwrap();
        channel.eof().await.unwrap();
        let mut output = Vec::new();
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    output.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    assert_eq!(exit_status, 0, "{}", String::from_utf8_lossy(&output));
                    break;
                }
                _ => {}
            }
        }
        assert!(String::from_utf8_lossy(&output).contains("STEP:sshd_reloaded"));
        // Lose the SSH transport before the client sends a commit.
        drop(channel);
        drop(session);

        let hardened_session = connect_after_reload(HOST, PORT, USER, key).await.unwrap();
        drop(hardened_session);

        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let password_works = match client::connect(HOST, PORT).await {
                    Ok(mut fresh) => client::auth_password(&mut fresh, USER, password).await.is_ok(),
                    Err(_) => false,
                };
                if password_works && !std::fs::exists(PENDING).unwrap() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .expect("the timer should restore password login after losing the transport");
        assert_eq!(std::fs::read(ssh_config).unwrap(), original_config);
        assert_eq!(std::fs::read(DROPIN_PATH).unwrap(), original_dropin);
    }

    #[tokio::test]
    #[ignore = "requires the disposable OpenSSH server configured by CI"]
    async fn detached_timer_rolls_back_without_systemd_run() {
        let key = std::env::var("TERMINALOX_TEST_KEY").unwrap();
        let password = std::env::var("TERMINALOX_TEST_PASSWORD").unwrap();
        assert_timer_rolls_back(&key, &password, "detached").await;
    }

    #[tokio::test]
    #[ignore = "requires the disposable systemd OpenSSH server configured by CI"]
    async fn hardening_restores_config_without_confirmation_and_commits_after_reconnect() {
        let key = std::env::var("TERMINALOX_TEST_KEY").unwrap();
        let password = std::env::var("TERMINALOX_TEST_PASSWORD").unwrap();
        assert_timer_rolls_back(&key, &password, "auto").await;

        let session = open_key_session(HOST, PORT, USER, &key).await.unwrap();
        let outcome = harden(session, (HOST, PORT, USER, &key), Some(&password))
            .await
            .unwrap();
        assert!(outcome.all_ok(), "{outcome:?}");
        assert!(!std::fs::exists(PENDING).unwrap());
    }
}
