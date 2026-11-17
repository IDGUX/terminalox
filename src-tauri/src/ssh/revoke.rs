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
    if key_line.is_empty() || key_line.contains(['\r', '\n']) {
        return Err(AppError::Other("invalid public key line".into()));
    }
    let escaped = key_line.replace('\'', "'\\''");

    let script = format!(
        r#"file="$HOME/.ssh/authorized_keys"
[ -f "$file" ] || exit 1
umask 077
tmp=$(mktemp "$HOME/.ssh/authorized_keys.terminalox.XXXXXXXX") || exit 1
trap 'rm -f -- "$tmp"' 0
grep -vxF -- '{escaped}' "$file" > "$tmp"
case $? in
    0|1) ;;
    *) exit 1 ;;
esac
mv -- "$tmp" "$file"
"#
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires the disposable OpenSSH server configured by CI"]
    async fn revocation_requires_a_working_password_login() {
        let host = "127.0.0.1";
        let port = 22222;
        let user = "terminalox-test";
        let key_path = std::env::var("TERMINALOX_TEST_KEY").expect("path to test key");
        let password = std::env::var("TERMINALOX_TEST_PASSWORD").expect("test account password");
        let public_key = std::fs::read_to_string(format!("{key_path}.pub")).unwrap();

        let rejected = revoke_with_password(host, port, user, "wrong password", &public_key).await;
        assert!(matches!(rejected, Err(AppError::AuthFailed)));

        let mut session = client::connect(host, port).await.unwrap();
        client::auth_key(&mut session, user, &key_path)
            .await
            .expect("the key must still work after a rejected fallback login");
        session
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await
            .unwrap();

        revoke_with_password(host, port, user, &password, &public_key)
            .await
            .expect("a working fallback login must allow key revocation");

        let mut session = client::connect(host, port).await.unwrap();
        let rejected = client::auth_key(&mut session, user, &key_path).await;
        assert!(matches!(rejected, Err(AppError::AuthFailed)));
        client::auth_password(&mut session, user, &password)
            .await
            .expect("the password must still work after revocation");
    }
}
