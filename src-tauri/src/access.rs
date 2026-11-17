//! Turns stored server entries into connectable targets with credentials.
//!
//! Missing secrets come back as `AppError::SecretRequired`; the frontend asks
//! the user, calls `provide_secret` and retries the command.

use std::collections::HashSet;

use russh::client::Handle;
use uuid::Uuid;

use crate::error::{AppError, AppResult, SecretRequest};
use crate::models::{AuthMode, KeyInfo, Server};
use crate::secrets::{SecretKind, Secrets};
use crate::ssh::client::{
    self, Credential, Hop, KeyCred, KnownHostsVerify, Target, MAX_JUMP_DEPTH,
};
use crate::storage::Store;

fn passphrase_required(key: &KeyInfo, wrong: bool) -> AppError {
    AppError::SecretRequired(SecretRequest {
        kind: "passphrase",
        id: key.id,
        label: key.name.clone(),
        wrong,
    })
}

fn password_required(server: &Server, wrong: bool) -> AppError {
    AppError::SecretRequired(SecretRequest {
        kind: "password",
        id: server.id,
        label: server.name.clone(),
        wrong,
    })
}

/// Loads a key, using a stored passphrase if the file is encrypted.
pub fn key_cred(secrets: &Secrets, key: &KeyInfo) -> AppResult<KeyCred> {
    let stored = if key.encrypted {
        secrets.get(SecretKind::KeyPassphrase, key.id)
    } else {
        None
    };
    match KeyCred::load(&key.private_path, stored.as_deref()) {
        Ok(k) => Ok(k),
        Err(AppError::KeyPassphrase { .. }) if stored.is_none() => {
            // Entry predates the `encrypted` flag: try a stored passphrase anyway.
            match secrets.get(SecretKind::KeyPassphrase, key.id) {
                Some(p) => KeyCred::load(&key.private_path, Some(&p)).map_err(|e| match e {
                    AppError::KeyPassphrase { .. } => passphrase_required(key, true),
                    other => other,
                }),
                None => Err(passphrase_required(key, false)),
            }
        }
        Err(AppError::KeyPassphrase { wrong }) => Err(passphrase_required(key, wrong)),
        Err(e) => Err(e),
    }
}

/// Checks a passphrase against the key file before it gets stored.
pub fn check_passphrase(key: &KeyInfo, passphrase: &str) -> AppResult<()> {
    match KeyCred::load(&key.private_path, Some(passphrase)) {
        Ok(_) => Ok(()),
        Err(AppError::KeyPassphrase { .. }) => Err(passphrase_required(key, true)),
        Err(e) => Err(e),
    }
}

/// Credential for logging into `server` itself. `password` overrides a stored one.
pub fn server_cred(
    store: &Store,
    secrets: &Secrets,
    server: &Server,
    password: Option<String>,
) -> AppResult<Credential> {
    match &server.auth {
        AuthMode::Password => password
            .filter(|p| !p.is_empty())
            .or_else(|| secrets.get(SecretKind::ServerPassword, server.id))
            .map(Credential::Password)
            .ok_or_else(|| password_required(server, false)),
        AuthMode::Key { key_id } => {
            let key = store.find_key(*key_id).ok_or(AppError::UnknownKey)?;
            Ok(Credential::Key(key_cred(secrets, key)?))
        }
        AuthMode::Agent => Ok(Credential::Agent),
    }
}

/// Network target of `server`, including its (resolved) jump chain.
pub fn target_for(store: &Store, secrets: &Secrets, server: &Server) -> AppResult<Target> {
    let mut visited = HashSet::new();
    target_inner(store, secrets, server, &mut visited)
}

fn target_inner(
    store: &Store,
    secrets: &Secrets,
    server: &Server,
    visited: &mut HashSet<Uuid>,
) -> AppResult<Target> {
    visited.insert(server.id);
    let jump = match server.jump_server_id {
        None => None,
        Some(jid) => {
            if visited.contains(&jid) {
                return Err(msg!("jump_loop", "The jump host chain contains a loop").into());
            }
            if visited.len() > MAX_JUMP_DEPTH {
                return Err(jump_too_long());
            }
            let j = store.find_server(jid).ok_or_else(|| {
                AppError::from(msg!(
                    "jump_missing",
                    "The jump host of \"{name}\" no longer exists",
                    name = server.name
                ))
            })?;
            Some(Box::new(Hop {
                target: target_inner(store, secrets, j, visited)?,
                user: j.user.clone(),
                cred: server_cred(store, secrets, j, None)?,
                server_id: j.id,
                name: j.name.clone(),
            }))
        }
    };
    Ok(Target {
        host: server.host.clone(),
        port: server.port,
        jump,
    })
}

fn jump_too_long() -> AppError {
    msg!(
        "jump_too_long",
        "Jump host chain is longer than {max} hops",
        max = MAX_JUMP_DEPTH
    )
    .into()
}

/// Validates that setting `jump` as jump host of `server_id` creates no cycle.
pub fn check_jump(store: &Store, server_id: Option<Uuid>, jump: Option<Uuid>) -> AppResult<()> {
    let mut cur = jump;
    let mut depth = 0;
    while let Some(id) = cur {
        if Some(id) == server_id {
            return Err(msg!(
                "jump_self",
                "A server cannot be its own jump host, not even indirectly"
            )
            .into());
        }
        depth += 1;
        if depth > MAX_JUMP_DEPTH {
            return Err(jump_too_long());
        }
        let s = store
            .find_server(id)
            .ok_or_else(|| AppError::from(msg!("jump_not_found", "Jump host not found")))?;
        cur = s.jump_server_id;
    }
    Ok(())
}

/// Everything needed to log into one server.
#[derive(Clone)]
pub struct Login {
    pub server: Server,
    pub target: Target,
    pub cred: Credential,
}

impl Login {
    pub fn new(
        store: &Store,
        secrets: &Secrets,
        server: &Server,
        password: Option<String>,
    ) -> AppResult<Self> {
        Ok(Login {
            server: server.clone(),
            target: target_for(store, secrets, server)?,
            cred: server_cred(store, secrets, server, password)?,
        })
    }

    /// Same target, different credential (e.g. a freshly deployed key).
    pub fn with_cred(&self, cred: Credential) -> Self {
        Login {
            cred,
            ..self.clone()
        }
    }

    pub async fn connect(&self) -> AppResult<Handle<KnownHostsVerify>> {
        client::connect_auth(&self.target, &self.server.user, &self.cred)
            .await
            .map_err(|e| match (e, &self.cred) {
                (AppError::AuthFailed, Credential::Password(_)) => {
                    password_required(&self.server, true)
                }
                (other, _) => other,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn server(name: &str, jump: Option<Uuid>) -> Server {
        Server {
            id: Uuid::new_v4(),
            name: name.into(),
            host: format!("{name}.example"),
            port: 22,
            user: "root".into(),
            auth: AuthMode::Agent,
            color: None,
            created_at: Utc::now(),
            last_used_at: None,
            hardened: false,
            hardened_at: None,
            folder_id: None,
            jump_server_id: jump,
        }
    }

    #[test]
    fn jump_chain_resolves_and_cycles_are_rejected() {
        let mut store = Store::default();
        let bastion = server("bastion", None);
        let inner = server("inner", Some(bastion.id));
        store.servers.push(bastion.clone());
        store.servers.push(inner.clone());
        let secrets = Secrets::default();

        let t = target_for(&store, &secrets, &inner).unwrap();
        assert_eq!(t.jump.as_ref().unwrap().target.host, "bastion.example");

        assert!(check_jump(&store, Some(bastion.id), Some(inner.id)).is_err());
        assert!(check_jump(&store, Some(bastion.id), Some(bastion.id)).is_err());
        assert!(check_jump(&store, None, Some(inner.id)).is_ok());
    }
}
