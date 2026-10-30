use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Config, Handle};
use russh::keys::known_hosts;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey};
use serde::Serialize;

use crate::error::{AppError, AppResult};

/// Host key the server presented but that is not (or no longer) in ~/.ssh/known_hosts.
/// Travels to the frontend, which asks the user and calls `trust_host_key`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyRejection {
    pub host: String,
    pub port: u16,
    /// true = known_hosts has a *different* key of the same type for this host.
    pub changed: bool,
    pub algorithm: String,
    pub fingerprint: String,
    pub key_openssh: String,
}

enum HostKeyStatus {
    Trusted,
    Unknown,
    Changed,
}

/// Same semantics as OpenSSH: any matching entry → trusted; only an entry of the
/// same algorithm with a different key → changed. Shares ~/.ssh/known_hosts with
/// the `ssh` CLI, so hosts already known there need no prompt (and vice versa).
fn host_key_status(host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
    let recorded = known_hosts::known_host_keys(host, port).unwrap_or_else(|e| {
        tracing::warn!("reading known_hosts for {host}:{port} failed: {e}");
        vec![]
    });
    if recorded.iter().any(|(_, k)| k == key) {
        HostKeyStatus::Trusted
    } else if recorded.iter().any(|(_, k)| k.algorithm() == key.algorithm()) {
        HostKeyStatus::Changed
    } else {
        HostKeyStatus::Unknown
    }
}

pub struct KnownHostsVerify {
    host: String,
    port: u16,
    rejected: Arc<Mutex<Option<HostKeyRejection>>>,
}

impl client::Handler for KnownHostsVerify {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        let changed = match host_key_status(&self.host, self.port, key) {
            HostKeyStatus::Trusted => return Ok(true),
            HostKeyStatus::Unknown => false,
            HostKeyStatus::Changed => true,
        };
        *self.rejected.lock().unwrap() = Some(HostKeyRejection {
            host: self.host.clone(),
            port: self.port,
            changed,
            algorithm: key.algorithm().as_str().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            key_openssh: key.to_openssh().unwrap_or_default(),
        });
        // Rejecting here aborts before any credentials are sent.
        Ok(false)
    }
}

pub async fn connect(host: &str, port: u16) -> AppResult<Handle<KnownHostsVerify>> {
    let config = Arc::new(Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    let rejected = Arc::new(Mutex::new(None));
    let handler = KnownHostsVerify {
        host: host.to_string(),
        port,
        rejected: rejected.clone(),
    };
    match client::connect(config, (host, port), handler).await {
        Ok(handle) => Ok(handle),
        Err(e) => match rejected.lock().unwrap().take() {
            Some(r) => Err(AppError::HostKey(r)),
            None => Err(e.into()),
        },
    }
}

pub fn trust_host_key(host: &str, port: u16, key_openssh: &str) -> AppResult<()> {
    let key = PublicKey::from_openssh(key_openssh)
        .map_err(|e| AppError::Other(format!("ungültiger Host-Key: {e}")))?;
    // Append only: an outdated entry may stay, but a matching entry wins (as in OpenSSH).
    known_hosts::learn_known_hosts(host, port, &key)?;
    Ok(())
}

pub async fn auth_password(
    session: &mut Handle<KnownHostsVerify>,
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
    session: &mut Handle<KnownHostsVerify>,
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
