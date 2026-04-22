use base64::prelude::*;
use russh::ChannelMsg;

use crate::error::{AppError, AppResult};
use crate::ssh::client::{self, NoopVerify};

pub const DROPIN_PATH: &str = "/etc/ssh/sshd_config.d/99-terminalox.conf";
const DROPIN_CONTENT: &str = "# Managed by terminalox — do not edit.\n\
                              # Removed automatically when the server is deleted from the app.\n\
                              PasswordAuthentication no\n\
                              KbdInteractiveAuthentication no\n";

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

/// Run the hardening flow over an authenticated session. Requires that the user
/// can elevate with `sudo -S` using `sudo_password`. Self-rolls-back on any failure.
pub async fn harden(
    session: russh::client::Handle<NoopVerify>,
    sudo_password: &str,
) -> AppResult<HardenOutcome> {
    let script = build_harden_script();
    let stdin = format!("{sudo_password}\n");
    let (exit, stdout, stderr) = exec_collect(session, &script, &stdin).await?;

    let outcome = parse_harden_stdout(&stdout, stderr.clone());

    if exit != 0 && !outcome.all_ok() {
        return Ok(HardenOutcome {
            stderr: format!("exit {exit}: {stderr}"),
            ..outcome
        });
    }
    Ok(outcome)
}

pub async fn unharden(
    session: russh::client::Handle<NoopVerify>,
    sudo_password: &str,
) -> AppResult<()> {
    let script = build_unharden_script();
    let stdin = format!("{sudo_password}\n");
    let (exit, _stdout, stderr) = exec_collect(session, &script, &stdin).await?;
    if exit != 0 {
        return Err(AppError::RemoteCommandFailed(format!(
            "unharden exit {exit}: {stderr}"
        )));
    }
    Ok(())
}

fn build_harden_script() -> String {
    let content_b64 = BASE64_STANDARD.encode(DROPIN_CONTENT);
    format!(
        r#"sudo -S -p '' bash -s <<'TERMINALOX_EOF'
set -u
DROPIN='{path}'
STAGE="$DROPIN.terminalox.new"
mkdir -p /etc/ssh/sshd_config.d
echo '{content}' | base64 -d > "$STAGE"
chmod 644 "$STAGE"
# Activate drop-in
mv "$STAGE" "$DROPIN"
echo 'STEP:dropin_installed'

# Validate full sshd config
if ! sshd -t 2>&1; then
  rm -f "$DROPIN"
  echo 'STEP:validation_failed' >&2
  exit 2
fi
echo 'STEP:sshd_validated'

# Reload (try known service names)
reload_ok=0
for svc in ssh sshd; do
  if systemctl reload "$svc" 2>/dev/null; then reload_ok=1; break; fi
done
if [ "$reload_ok" -eq 0 ]; then
  for svc in ssh sshd; do
    if service "$svc" reload 2>/dev/null; then reload_ok=1; break; fi
  done
fi
if [ "$reload_ok" -eq 0 ]; then
  rm -f "$DROPIN"
  echo 'STEP:reload_failed' >&2
  exit 3
fi
echo 'STEP:sshd_reloaded'

# Verify effective config
if sshd -T 2>/dev/null | grep -qiE '^passwordauthentication no'; then
  echo 'STEP:password_auth_off'
else
  # undo
  rm -f "$DROPIN"
  for svc in ssh sshd; do systemctl reload "$svc" 2>/dev/null || service "$svc" reload 2>/dev/null; done
  echo 'STEP:password_auth_verify_failed' >&2
  exit 4
fi
TERMINALOX_EOF"#,
        path = DROPIN_PATH,
        content = content_b64
    )
}

fn build_unharden_script() -> String {
    format!(
        r#"sudo -S -p '' bash -s <<'TERMINALOX_EOF'
set -u
DROPIN='{path}'
if [ -f "$DROPIN" ]; then rm -f "$DROPIN"; fi
for svc in ssh sshd; do systemctl reload "$svc" 2>/dev/null && exit 0; done
for svc in ssh sshd; do service "$svc" reload 2>/dev/null && exit 0; done
# If no reload worked, still report success — drop-in is gone, new sshd behavior will apply on next config read.
exit 0
TERMINALOX_EOF"#,
        path = DROPIN_PATH
    )
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
    session: russh::client::Handle<NoopVerify>,
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
) -> AppResult<russh::client::Handle<NoopVerify>> {
    let mut session = client::connect(host, port).await?;
    client::auth_key(&mut session, user, key_path).await?;
    Ok(session)
}
