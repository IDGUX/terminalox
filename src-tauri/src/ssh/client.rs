use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Config, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::agent::AgentIdentity;
use russh::keys::known_hosts;
use russh::keys::{
    load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate,
};
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::{AppError, AppResult};

/// How long establishing TCP + SSH handshake to one hop may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Upper bound for jump chains; also stops cycles that slipped past validation.
pub const MAX_JUMP_DEPTH: usize = 5;

/// A decrypted private key, ready to sign.
#[derive(Debug, Clone)]
pub struct KeyCred {
    key: Arc<russh::keys::PrivateKey>,
}

impl KeyCred {
    /// Loads (and decrypts) a private key file. A missing or wrong passphrase
    /// yields `AppError::KeyPassphrase` so the caller can ask for it.
    pub fn load(path: &str, passphrase: Option<&str>) -> AppResult<Self> {
        let key = match load_secret_key(path, passphrase) {
            Ok(k) => k,
            Err(russh::keys::Error::KeyIsEncrypted) => {
                return Err(AppError::KeyPassphrase { wrong: false })
            }
            Err(e) if passphrase.is_some() && is_decrypt_error(&e) => {
                return Err(AppError::KeyPassphrase { wrong: true })
            }
            Err(e) => return Err(e.into()),
        };
        Ok(KeyCred { key: Arc::new(key) })
    }
}

/// Wrong passphrases surface as generic decoding errors in ssh-key.
fn is_decrypt_error(e: &russh::keys::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    ["decrypt", "crypto", "passphrase", "checkint", "cipher"]
        .iter()
        .any(|w| msg.contains(w))
}

/// How to log into one hop.
#[derive(Debug, Clone)]
pub enum Credential {
    Password(String),
    Key(KeyCred),
    Agent,
}

/// A jump host on the way to the target, with its own login.
#[derive(Debug, Clone)]
pub struct Hop {
    pub target: Target,
    pub user: String,
    pub cred: Credential,
    /// terminalox server entry of the jump host, for error messages and prompts.
    pub server_id: uuid::Uuid,
    pub name: String,
}

/// Where to connect. `jump` makes the TCP connection run through an
/// authenticated SSH session on another host (like OpenSSH `ProxyJump`).
#[derive(Debug, Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub jump: Option<Box<Hop>>,
}

#[cfg(test)]
impl Target {
    pub fn direct(host: &str, port: u16) -> Self {
        Target {
            host: host.to_string(),
            port,
            jump: None,
        }
    }
}

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
    } else if recorded
        .iter()
        .any(|(_, k)| k.algorithm() == key.algorithm())
    {
        HostKeyStatus::Changed
    } else {
        HostKeyStatus::Unknown
    }
}

pub struct KnownHostsVerify {
    host: String,
    port: u16,
    rejected: Arc<Mutex<Option<HostKeyRejection>>>,
    /// Keeps the jump session alive as long as this connection exists.
    _via: Option<Handle<KnownHostsVerify>>,
}

impl client::Handler for KnownHostsVerify {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // Certificate authorities are not part of terminalox's known_hosts policy.
        let PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return Ok(false);
        };
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

fn ssh_config() -> Arc<Config> {
    Arc::new(Config {
        // Keepalives detect dead peers; no inactivity cutoff for idle shells.
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 4,
        ..Default::default()
    })
}

/// Opens an SSH transport to `target`, tunnelling through its jump chain.
/// Host keys of every hop are checked against `~/.ssh/known_hosts`.
pub fn connect(
    target: &Target,
) -> Pin<Box<dyn Future<Output = AppResult<Handle<KnownHostsVerify>>> + Send + '_>> {
    Box::pin(connect_depth(target, 0))
}

async fn connect_depth(target: &Target, depth: usize) -> AppResult<Handle<KnownHostsVerify>> {
    if depth > MAX_JUMP_DEPTH {
        return Err(msg!(
            "jump_too_long",
            "Jump host chain is longer than {max} hops",
            max = MAX_JUMP_DEPTH
        )
        .into());
    }
    let rejected = Arc::new(Mutex::new(None));
    let host_key_error =
        |e: AppError, rejected: &Arc<Mutex<Option<HostKeyRejection>>>| match rejected
            .lock()
            .unwrap()
            .take()
        {
            Some(r) => AppError::HostKey(r),
            None => e,
        };

    let Some(hop) = &target.jump else {
        let handler = KnownHostsVerify {
            host: target.host.clone(),
            port: target.port,
            rejected: rejected.clone(),
            _via: None,
        };
        let fut = client::connect(ssh_config(), (target.host.as_str(), target.port), handler);
        return match tokio::time::timeout(CONNECT_TIMEOUT, fut).await {
            Ok(Ok(handle)) => Ok(handle),
            Ok(Err(e)) => Err(host_key_error(e.into(), &rejected)),
            Err(_) => Err(msg!(
                "connect_timeout",
                "Timed out connecting to {host}:{port}",
                host = target.host,
                port = target.port
            )
            .into()),
        };
    };

    let mut jump = Box::pin(connect_depth(&hop.target, depth + 1)).await?;
    authenticate(&mut jump, &hop.user, &hop.cred)
        .await
        .map_err(|e| match (e, &hop.cred) {
            (AppError::AuthFailed, Credential::Password(_)) => {
                AppError::SecretRequired(crate::error::SecretRequest {
                    kind: "password",
                    id: hop.server_id,
                    label: hop.name.clone(),
                    wrong: true,
                })
            }
            (AppError::AuthFailed, _) => msg!(
                "jump_auth_failed",
                "Login to jump host \"{name}\" failed",
                name = hop.name
            )
            .into(),
            (other, _) => other,
        })?;
    let channel = jump
        .channel_open_direct_tcpip(target.host.clone(), target.port as u32, "127.0.0.1", 0)
        .await
        .map_err(|e| {
            AppError::from(msg!(
                "jump_unreachable",
                "Jump host {jump} cannot reach {host}:{port}: {detail}",
                jump = hop.name,
                host = target.host,
                port = target.port,
                detail = e
            ))
        })?;
    let handler = KnownHostsVerify {
        host: target.host.clone(),
        port: target.port,
        rejected: rejected.clone(),
        _via: Some(jump),
    };
    let fut = client::connect_stream(ssh_config(), channel.into_stream(), handler);
    match tokio::time::timeout(CONNECT_TIMEOUT, fut).await {
        Ok(Ok(handle)) => Ok(handle),
        Ok(Err(e)) => Err(host_key_error(e.into(), &rejected)),
        Err(_) => Err(msg!(
            "connect_timeout",
            "Timed out connecting to {host}:{port}",
            host = target.host,
            port = target.port
        )
        .into()),
    }
}

/// Logs in with any supported credential.
pub async fn authenticate(
    session: &mut Handle<KnownHostsVerify>,
    user: &str,
    cred: &Credential,
) -> AppResult<()> {
    match cred {
        Credential::Password(pw) => auth_password(session, user, pw).await,
        Credential::Key(key) => auth_key(session, user, key).await,
        Credential::Agent => auth_agent(session, user).await,
    }
}

/// Connect + authenticate in one step.
pub async fn connect_auth(
    target: &Target,
    user: &str,
    cred: &Credential,
) -> AppResult<Handle<KnownHostsVerify>> {
    let mut session = connect(target).await?;
    authenticate(&mut session, user, cred).await?;
    Ok(session)
}

pub fn trust_host_key(host: &str, port: u16, key_openssh: &str) -> AppResult<()> {
    let key = PublicKey::from_openssh(key_openssh)
        .map_err(|e| AppError::Other(format!("invalid host key: {e}")))?;
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
    cred: &KeyCred,
) -> AppResult<()> {
    let best_hash = session.best_supported_rsa_hash().await?.flatten();
    let res = session
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(cred.key.clone(), best_hash),
        )
        .await?;
    if !res.success() {
        return Err(AppError::AuthFailed);
    }
    Ok(())
}

/// Tries every identity of the running ssh-agent.
pub async fn auth_agent(session: &mut Handle<KnownHostsVerify>, user: &str) -> AppResult<()> {
    #[cfg(unix)]
    {
        let agent = AgentClient::connect_env()
            .await
            .map_err(agent_unavailable)?;
        auth_with_agent(session, user, agent).await
    }
    #[cfg(windows)]
    {
        match AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
            Ok(agent) => auth_with_agent(session, user, agent).await,
            Err(_) => {
                let agent = AgentClient::connect_pageant()
                    .await
                    .map_err(agent_unavailable)?;
                auth_with_agent(session, user, agent).await
            }
        }
    }
}

fn agent_unavailable(e: russh::keys::Error) -> AppError {
    msg!(
        "agent_unavailable",
        "No ssh-agent reachable ({detail}). Start an agent or set SSH_AUTH_SOCK.",
        detail = e
    )
    .into()
}

async fn auth_with_agent<S>(
    session: &mut Handle<KnownHostsVerify>,
    user: &str,
    mut agent: AgentClient<S>,
) -> AppResult<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let identities = agent.request_identities().await?;
    let keys: Vec<PublicKey> = identities
        .into_iter()
        .filter_map(|id| match id {
            AgentIdentity::PublicKey { key, .. } => Some(key),
            _ => None,
        })
        .collect();
    if keys.is_empty() {
        return Err(msg!(
            "agent_empty",
            "The ssh-agent has no keys loaded (ssh-add -l)."
        )
        .into());
    }
    let rsa_hash = session.best_supported_rsa_hash().await?.flatten();
    for key in keys {
        let hash = if matches!(key.algorithm(), russh::keys::Algorithm::Rsa { .. }) {
            rsa_hash
        } else {
            None
        };
        match session
            .authenticate_publickey_with(user, key, hash, &mut agent)
            .await
        {
            Ok(r) if r.success() => return Ok(()),
            Ok(_) => continue,
            Err(e) => tracing::debug!("agent signing failed: {e}"),
        }
    }
    Err(AppError::AuthFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::ssh_key::{Algorithm, LineEnding, PrivateKey};

    #[test]
    fn encrypted_keys_need_the_right_passphrase() {
        let mut rng = rand_core::UnwrapErr(rand::rngs::SysRng);
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap();
        let enc = key.encrypt(&mut rng, "correct horse").unwrap();
        let dir = std::env::temp_dir().join(format!("terminalox-key-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("k");
        std::fs::write(&path, enc.to_openssh(LineEnding::LF).unwrap().as_bytes()).unwrap();
        let p = path.to_str().unwrap();

        assert!(matches!(
            KeyCred::load(p, None),
            Err(AppError::KeyPassphrase { wrong: false })
        ));
        assert!(matches!(
            KeyCred::load(p, Some("wrong")),
            Err(AppError::KeyPassphrase { wrong: true })
        ));
        assert!(KeyCred::load(p, Some("correct horse")).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod integration {
    use super::*;

    /// Tunnels to the CI sshd through itself, which exercises direct-tcpip,
    /// `connect_stream` and keeping the jump session alive.
    #[tokio::test]
    #[ignore = "requires the disposable OpenSSH server configured by CI"]
    async fn connects_through_jump_host() {
        let key_path = std::env::var("TERMINALOX_TEST_KEY").expect("path to test key");
        let user =
            std::env::var("TERMINALOX_TEST_USER").unwrap_or_else(|_| "terminalox-test".into());
        let key = KeyCred::load(&key_path, None).unwrap();
        let target = Target {
            host: "127.0.0.1".into(),
            port: 22222,
            jump: Some(Box::new(Hop {
                target: Target::direct("127.0.0.1", 22222),
                user: user.clone(),
                cred: Credential::Key(key.clone()),
                server_id: uuid::Uuid::new_v4(),
                name: "jump".into(),
            })),
        };
        let session = connect_auth(&target, &user, &Credential::Key(key))
            .await
            .expect("login through the jump host");
        let mut channel = session.channel_open_session().await.unwrap();
        channel.exec(true, "echo through-jump").await.unwrap();
        let mut out = Vec::new();
        while let Some(msg) = channel.wait().await {
            if let russh::ChannelMsg::Data { data } = msg {
                out.extend_from_slice(&data);
            }
        }
        assert_eq!(String::from_utf8_lossy(&out).trim(), "through-jump");
    }

    /// Needs `SSH_AUTH_SOCK` pointing at an agent that holds the test key.
    #[tokio::test]
    #[ignore = "requires the CI sshd and an ssh-agent holding the test key"]
    async fn authenticates_with_agent() {
        let user =
            std::env::var("TERMINALOX_TEST_USER").unwrap_or_else(|_| "terminalox-test".into());
        let target = Target::direct("127.0.0.1", 22222);
        connect_auth(&target, &user, &Credential::Agent)
            .await
            .expect("agent login");
    }
}
