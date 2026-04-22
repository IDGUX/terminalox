use russh::ChannelMsg;

use crate::error::{AppError, AppResult};
use crate::ssh::client;

pub async fn deploy_public_key(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    public_key_openssh: &str,
) -> AppResult<()> {
    let public_key_trimmed = public_key_openssh.trim();
    if public_key_trimmed.is_empty() {
        return Err(AppError::Other("empty public key".into()));
    }

    let escaped = public_key_trimmed.replace('\'', "'\\''");
    let script = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
         touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && \
         grep -qxF -- '{0}' ~/.ssh/authorized_keys || echo '{0}' >> ~/.ssh/authorized_keys",
        escaped
    );

    let mut session = client::connect(host, port).await?;
    client::auth_password(&mut session, user, password).await?;

    let mut channel = session.channel_open_session().await?;
    channel.exec(true, script.as_bytes()).await?;

    let mut stderr_buf: Vec<u8> = Vec::new();
    let mut exit: Option<u32> = None;

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data: _ } => {}
            ChannelMsg::ExtendedData { ref data, ext } if ext == 1 => {
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
