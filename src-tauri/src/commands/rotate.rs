//! Key rotation: replace a server's key without ever losing access.
//!
//! Order matters: the new key is added and proven to work before the server
//! entry switches to it, and the old key is only revoked over a session that
//! was opened with the new key.

use chrono::Utc;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::access::{self, Login};
use crate::error::{AppError, AppResult};
use crate::keygen;
use crate::models::{AuthMode, KeyInfo};
use crate::msg::Msg;
use crate::secrets::SecretKind;
use crate::ssh::client::Credential;
use crate::ssh::{deploy, revoke};
use crate::ssh_config;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RotateReport {
    pub new_key: KeyInfo,
    pub old_key_revoked: bool,
    pub old_key_removed_locally: bool,
    /// Other servers still use the old key, so it stays on this machine.
    pub old_key_still_used_by: Vec<String>,
    pub warnings: Vec<Msg>,
}

fn rotated_name(server_name: &str, existing: &[KeyInfo]) -> String {
    let base = format!("{}_{}", server_name, Utc::now().format("%Y%m%d"));
    let base: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let taken = |n: &str| {
        existing.iter().any(|k| k.name == n)
            || keygen::ssh_dir()
                .map(|d| d.join(format!("terminalox_{n}")).exists())
                .unwrap_or(false)
    };
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|i| format!("{base}_{i}"))
        .find(|n| !taken(n))
        .expect("infinite iterator")
}

#[tauri::command]
pub async fn rotate_key(server_id: Uuid, state: State<'_, AppState>) -> AppResult<RotateReport> {
    let (old_login, old_key, new_name) = {
        let store = state.store.read().await;
        let server = store
            .find_server(server_id)
            .ok_or(AppError::UnknownServer)?;
        let AuthMode::Key { key_id } = server.auth else {
            return Err(msg!(
                "rotate_key_only",
                "Key rotation is only possible for servers with a terminalox key."
            )
            .into());
        };
        let old_key = store.find_key(key_id).ok_or(AppError::UnknownKey)?.clone();
        let login = Login::new(&store, &state.secrets, server, None)?;
        (login, old_key, rotated_name(&server.name, &store.keys))
    };

    // 1. New key on disk (not yet in the store).
    let new_key = keygen::generate_ed25519(&new_name, None)?;
    let cleanup_local = |k: &KeyInfo| {
        let _ = keygen::delete_key_files(k);
    };

    // 2. Add it to authorized_keys over the current key.
    let added = async {
        let session = old_login.connect().await?;
        deploy::deploy_on_authenticated(session, &new_key.public_openssh).await
    }
    .await;
    if let Err(e) = added {
        cleanup_local(&new_key);
        return Err(e);
    }

    // 3. Prove the new key works on a fresh connection.
    let new_cred = match access::key_cred(&state.secrets, &new_key) {
        Ok(c) => c,
        Err(e) => {
            cleanup_local(&new_key);
            return Err(e);
        }
    };
    let new_login = old_login.with_cred(Credential::Key(new_cred));
    if let Err(e) = new_login.connect().await {
        // Undo step 2 over the old key, which still works.
        let _ = revoke::revoke_keeping_other(&old_login, &new_key.public_openssh).await;
        cleanup_local(&new_key);
        return Err(msg!(
            "rotate_new_key_rejected",
            "The server did not accept the new key ({detail}). The old key stays in use.",
            detail = e.to_msg().text
        )
        .into());
    }

    // 4. Switch the server entry.
    let still_used_by: Vec<String> = {
        let mut store = state.store.write().await;
        store.keys.push(new_key.clone());
        if let Some(s) = store.find_server_mut(server_id) {
            s.auth = AuthMode::Key { key_id: new_key.id };
        }
        store.save(&state.store_file())?;
        if let Err(e) = ssh_config::sync_if_managed(&store) {
            tracing::warn!("ssh config sync after rotation failed: {e}");
        }
        store
            .servers
            .iter()
            .filter(|s| matches!(s.auth, AuthMode::Key { key_id } if key_id == old_key.id))
            .map(|s| s.name.clone())
            .collect()
    };

    let mut report = RotateReport {
        new_key: new_key.clone(),
        old_key_revoked: false,
        old_key_removed_locally: false,
        old_key_still_used_by: still_used_by.clone(),
        warnings: vec![],
    };

    // 5. Revoke the old key over the new one.
    match revoke::revoke_keeping_other(&new_login, &old_key.public_openssh).await {
        Ok(()) => report.old_key_revoked = true,
        Err(e) => report.warnings.push(msg!(
            "rotate_revoke_failed",
            "The old key could not be removed from the server: {detail}. It stays in authorized_keys.",
            detail = e.to_msg().text
        )),
    }

    // 6. Drop the old key locally if nothing else uses it.
    if report.old_key_revoked && still_used_by.is_empty() {
        let mut store = state.store.write().await;
        match keygen::delete_key_files(&old_key) {
            Ok(()) => {
                store.keys.retain(|k| k.id != old_key.id);
                state.secrets.forget(SecretKind::KeyPassphrase, old_key.id);
                store.save(&state.store_file())?;
                report.old_key_removed_locally = true;
            }
            Err(e) => report.warnings.push(msg!(
                "rotate_delete_failed",
                "Old key files could not be deleted: {detail}",
                detail = e
            )),
        }
    }

    Ok(report)
}
