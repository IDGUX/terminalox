use russh::ChannelMsg;

use crate::error::{AppError, AppResult};
use crate::ssh::client::{self, KnownHostsVerify};

/// Remove a specific public key line from `~/.ssh/authorized_keys` on the remote host.
///
/// Matches exact-line (grep -vxF) to avoid clobbering similar-but-distinct keys.
pub async fn revoke_on_authenticated(
    session: russh::client::Handle<KnownHostsVerify>,
    public_key_openssh: &str,
) -> AppResult<()> {
    let key_line = public_key_openssh.trim();
    if key_line.is_empty() {
        return Err(AppError::Other("empty public key".into()));
    }
    let escaped = key_line.replace('\'', "'\\''");

    // Atomic: write filtered copy to tmp, then rename.
    let script = format!(
        "if [ -f ~/.ssh/authorized_keys ]; then \
           grep -vxF -- '{0}' ~/.ssh/authorized_keys > ~/.ssh/authorized_keys.terminalox.tmp || true; \
           mv ~/.ssh/authorized_keys.terminalox.tmp ~/.ssh/authorized_keys; \
           chmod 600 ~/.ssh/authorized_keys; \
         fi",
        escaped
    );

    let mut channel = session.channel_open_session().await?;
    channel.exec(true, script.as_bytes()).await?;

    let mut stderr_buf: Vec<u8> = Vec::new();
    let mut exit: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::ExtendedData { ref data, ext } if ext == 1 => {
                stderr_buf.extend_from_slice(data);
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit = Some(exit_status);
            }
            _ => {}
        }
    }
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;

    let code = exit.unwrap_or(1);
    if code != 0 {
        let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
        return Err(AppError::RemoteCommandFailed(format!(
            "revoke exit {code}: {stderr}"
        )));
    }
    Ok(())
}

pub async fn revoke_with_password(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    public_key_openssh: &str,
) -> AppResult<()> {
    let mut session = client::connect(host, port).await?;
    client::auth_password(&mut session, user, password).await?;
    revoke_on_authenticated(session, public_key_openssh).await
}

pub async fn revoke_with_key(
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    public_key_openssh: &str,
) -> AppResult<()> {
    let mut session = client::connect(host, port).await?;
    client::auth_key(&mut session, user, key_path).await?;
    revoke_on_authenticated(session, public_key_openssh).await
}
