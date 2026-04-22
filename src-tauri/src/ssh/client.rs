use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Config, Handle};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};

use crate::error::{AppError, AppResult};

pub struct NoopVerify;

impl client::Handler for NoopVerify {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // MVP: accept-any. TODO: known_hosts file + first-use trust prompt.
        Ok(true)
    }
}

pub async fn connect(host: &str, port: u16) -> AppResult<Handle<NoopVerify>> {
    let config = Arc::new(Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    let handle = client::connect(config, (host, port), NoopVerify).await?;
    Ok(handle)
}

pub async fn auth_password(
    session: &mut Handle<NoopVerify>,
    user: &str,
    password: &str,
) -> AppResult<()> {
    let res = session.authenticate_password(user, password).await?;
    if !res.success() {
        return Err(AppError::AuthFailed);
    }
    Ok(())
}

pub async fn auth_key(
    session: &mut Handle<NoopVerify>,
    user: &str,
    key_path: &str,
) -> AppResult<()> {
    let key = load_secret_key(key_path, None)?;
    let best_hash = session.best_supported_rsa_hash().await?.flatten();
    let res = session
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), best_hash))
        .await?;
    if !res.success() {
        return Err(AppError::AuthFailed);
    }
    Ok(())
}
