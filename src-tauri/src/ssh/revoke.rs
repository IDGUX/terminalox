use russh::ChannelMsg;

use crate::access::Login;
use crate::error::{AppError, AppResult};
use crate::ssh::client::KnownHostsVerify;

/// Remove a specific public key line from `~/.ssh/authorized_keys` on the remote host.
///
/// Matches exact-line (grep -vxF) to avoid clobbering similar-but-distinct keys.
///
/// With `require_other_key`, the file is left untouched unless at least one
/// other key line remains, so a key-only account keeps a way in.
pub async fn revoke_on_authenticated(
    session: russh::client::Handle<KnownHostsVerify>,
    public_key_openssh: &str,
    require_other_key: bool,
) -> AppResult<()> {
    let key_line = public_key_openssh.trim();
    if key_line.is_empty() || key_line.contains(['\r', '\n']) {
        return Err(msg!("invalid_public_key", "Invalid public key line").into());
    }
    let escaped = key_line.replace('\'', "'\\''");
    let require_other = if require_other_key { "1" } else { "0" };

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
if [ '{require_other}' = 1 ] && ! grep -qvE '^[[:space:]]*(#|$)' "$tmp"; then
    echo 'NO_OTHER_KEY' >&2
    exit 3
fi
mv -- "$tmp" "$file"
"#
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
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;

    let code = exit.unwrap_or(1);
    if code == 3 {
        return Err(msg!(
            "revoke_last_key",
            "No other SSH key is set up on the server. Without a password you would lock yourself out, so the key stays."
        )
        .into());
    }
    if code != 0 {
        let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
        return Err(AppError::RemoteCommandFailed(format!(
            "revoke exit {code}: {stderr}"
        )));
    }
    Ok(())
}

/// Revoke after logging in with `login`'s password: proves the fallback works.
pub async fn revoke_with_password(login: &Login, public_key_openssh: &str) -> AppResult<()> {
    let session = login.connect().await?;
    revoke_on_authenticated(session, public_key_openssh, false).await
}

/// Revoke over a key login, for accounts without a password. Refuses when
/// no other key would remain in `authorized_keys`.
pub async fn revoke_keeping_other(login: &Login, public_key_openssh: &str) -> AppResult<()> {
    let session = login.connect().await?;
    revoke_on_authenticated(session, public_key_openssh, true).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AuthMode, Server};
    use crate::ssh::client::{self, Credential, KeyCred, Target};

    fn login(cred: Credential) -> Login {
        Login {
            server: Server {
                id: uuid::Uuid::new_v4(),
                name: "ci".into(),
                host: "127.0.0.1".into(),
                port: 22222,
                user: "terminalox-test".into(),
                auth: AuthMode::Agent,
                color: None,
                created_at: chrono::Utc::now(),
                last_used_at: None,
                hardened: false,
                hardened_at: None,
                folder_id: None,
                jump_server_id: None,
            },
            target: Target::direct("127.0.0.1", 22222),
            cred,
        }
    }

    #[tokio::test]
    #[ignore = "requires the disposable OpenSSH server configured by CI"]
    async fn revocation_requires_a_working_password_login() {
        let host = "127.0.0.1";
        let port = 22222;
        let user = "terminalox-test";
        let key_path = std::env::var("TERMINALOX_TEST_KEY").expect("path to test key");
        let password = std::env::var("TERMINALOX_TEST_PASSWORD").expect("test account password");
        let public_key = std::fs::read_to_string(format!("{key_path}.pub")).unwrap();

        // The fixture's authorized_keys holds only this key: key-only revocation must refuse.
        let key = KeyCred::load(&key_path, None).unwrap();
        let target = Target::direct(host, port);
        let refused = revoke_keeping_other(&login(Credential::Key(key.clone())), &public_key).await;
        assert!(refused.is_err(), "revoking the last key must be refused");

        let wrong = login(Credential::Password("wrong password".into()));
        let rejected = revoke_with_password(&wrong, &public_key).await;
        assert!(matches!(rejected, Err(AppError::SecretRequired(_))));

        let mut session = client::connect(&target).await.unwrap();
        client::auth_key(&mut session, user, &key)
            .await
            .expect("the key must still work after a rejected fallback login");
        session
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await
            .unwrap();

        revoke_with_password(&login(Credential::Password(password.clone())), &public_key)
            .await
            .expect("a working fallback login must allow key revocation");

        let mut session = client::connect(&target).await.unwrap();
        let rejected = client::auth_key(&mut session, user, &key).await;
        assert!(matches!(rejected, Err(AppError::AuthFailed)));
        client::auth_password(&mut session, user, &password)
            .await
            .expect("the password must still work after revocation");
    }
}
