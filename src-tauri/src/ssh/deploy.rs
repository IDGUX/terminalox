use russh::ChannelMsg;

use crate::access::Login;
use crate::error::{AppError, AppResult};
use crate::ssh::client::KnownHostsVerify;
use crate::validate;

/// Logs in with `login` (usually the password) and appends the key line.
pub async fn deploy_public_key(login: &Login, public_key_openssh: &str) -> AppResult<()> {
    let session = login.connect().await?;
    deploy_on_authenticated(session, public_key_openssh).await
}

/// Appends a public key line to `~/.ssh/authorized_keys` unless it is already there.
pub async fn deploy_on_authenticated(
    session: russh::client::Handle<KnownHostsVerify>,
    public_key_openssh: &str,
) -> AppResult<()> {
    let line = validate::public_key_line(public_key_openssh)?;
    let escaped = line.replace('\'', "'\\''");
    let script = format!(
        "umask 077 && mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
         touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && \
         {{ grep -qxF -- '{0}' ~/.ssh/authorized_keys || printf '%s\\n' '{0}' >> ~/.ssh/authorized_keys; }}",
        escaped
    );

    let mut channel = session.channel_open_session().await?;
    channel.exec(true, script.as_bytes()).await?;

    let mut stderr_buf: Vec<u8> = Vec::new();
    let mut exit: Option<u32> = None;

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                stderr_buf.extend_from_slice(data);
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit = Some(exit_status);
            }
            _ => {}
        }
    }

    session
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .ok();

    let code = exit.unwrap_or(1);
    if code != 0 {
        let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
        return Err(AppError::RemoteCommandFailed(format!(
            "exit {code}: {stderr}"
        )));
    }
    Ok(())
}
