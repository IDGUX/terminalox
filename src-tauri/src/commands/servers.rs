use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::access::{self, Login};
use crate::error::{AppError, AppResult};
use crate::keygen;
use crate::models::{AuthMode, NewServerInput, Server, UpdateServerInput};
use crate::secrets::SecretKind;
use crate::ssh::client::Credential;
use crate::ssh::{harden, revoke};
use crate::ssh_config;
use crate::state::AppState;
use crate::storage::Store;
use crate::validate;

fn check_auth_refs(store: &Store, auth: &AuthMode) -> AppResult<()> {
    if let AuthMode::Key { key_id } = auth {
        store.find_key(*key_id).ok_or(AppError::UnknownKey)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_servers(state: State<'_, AppState>) -> AppResult<Vec<Server>> {
    let store = state.store.read().await;
    Ok(store.servers.clone())
}

#[tauri::command]
pub async fn add_server(input: NewServerInput, state: State<'_, AppState>) -> AppResult<Server> {
    let mut server = Server {
        id: Uuid::new_v4(),
        name: input.name,
        host: input.host,
        port: input.port,
        user: input.user,
        auth: input.auth,
        color: input.color,
        created_at: Utc::now(),
        last_used_at: None,
        hardened: false,
        hardened_at: None,
        folder_id: input.folder_id,
        jump_server_id: input.jump_server_id,
    };
    validate::server(&mut server)?;
    let mut store = state.store.write().await;
    validate::unique_name(&store, &server.name, None)?;
    if let Some(fid) = server.folder_id {
        store.find_folder(fid).ok_or(AppError::UnknownFolder)?;
    }
    check_auth_refs(&store, &server.auth)?;
    access::check_jump(&store, Some(server.id), server.jump_server_id)?;
    store.servers.push(server.clone());
    store.save(&state.store_file())?;
    Ok(server)
}

#[tauri::command]
pub async fn update_server(
    input: UpdateServerInput,
    state: State<'_, AppState>,
) -> AppResult<Server> {
    let mut store = state.store.write().await;
    let mut updated = store
        .find_server(input.id)
        .ok_or(AppError::UnknownServer)?
        .clone();
    if let Some(v) = input.name {
        updated.name = v;
    }
    if let Some(v) = input.host {
        updated.host = v;
    }
    if let Some(v) = input.port {
        updated.port = v;
    }
    if let Some(v) = input.user {
        updated.user = v;
    }
    if let Some(v) = input.color {
        updated.color = v;
    }
    if let Some(v) = input.auth {
        updated.auth = v;
    }
    if let Some(v) = input.jump_server_id {
        updated.jump_server_id = v;
    }
    validate::server(&mut updated)?;
    validate::unique_name(&store, &updated.name, Some(updated.id))?;
    check_auth_refs(&store, &updated.auth)?;
    access::check_jump(&store, Some(updated.id), updated.jump_server_id)?;

    let server = store
        .find_server_mut(input.id)
        .ok_or(AppError::UnknownServer)?;
    *server = updated.clone();
    store.save(&state.store_file())?;
    Ok(updated)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteServerOptions {
    pub id: Uuid,
    /// Attempt to remove the deployed public key from the remote host's authorized_keys.
    #[serde(default)]
    pub cleanup_remote: bool,
    /// On local-only deletion, also delete the assigned key if no server references it.
    #[serde(default)]
    pub delete_unused_local_key: bool,
    /// Password used to verify fallback access before revoking a key.
    #[serde(default)]
    pub password: Option<String>,
    /// Sudo password used by unharden when the remote user needs one (Debian/Ubuntu default).
    /// Unused if the remote user is root or has NOPASSWD sudo.
    #[serde(default)]
    pub sudo_password: Option<String>,
    /// Without a password: the user confirmed another key gives access to the
    /// account. The revocation still refuses if no other key line remains.
    #[serde(default)]
    pub other_key_confirmed: bool,
    /// Delete only the local server entry and preserve its key.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeleteServerReport {
    pub server_removed: bool,
    pub remote_unharden_attempted: bool,
    pub remote_unharden_succeeded: bool,
    pub remote_unharden_error: Option<String>,
    pub remote_cleanup_attempted: bool,
    pub remote_cleanup_succeeded: bool,
    pub remote_cleanup_error: Option<String>,
    pub local_key_removed: bool,
    pub local_key_error: Option<String>,
    pub key_still_referenced: bool,
    /// Key-only cleanup leaves the terminalox hardening on the server.
    pub hardening_kept: bool,
    /// Revocation failed after unhardening and the hardening was re-applied.
    pub hardening_restored: bool,
    pub hardening_restore_error: Option<String>,
}

async fn restore_hardening(key_login: &Login, sudo_password: Option<&str>) -> Result<(), String> {
    let session = key_login.connect().await.map_err(|e| e.to_string())?;
    let outcome = harden::harden(session, key_login, sudo_password)
        .await
        .map_err(|e| e.to_string())?;
    if outcome.all_ok() {
        Ok(())
    } else {
        Err(outcome.stderr)
    }
}

/// Record that the server is no longer hardened so the UI does not claim otherwise.
async fn mark_unhardened(state: &State<'_, AppState>, id: Uuid) -> AppResult<()> {
    let mut store = state.store.write().await;
    if let Some(s) = store.find_server_mut(id) {
        s.hardened = false;
    }
    store.save(&state.store_file())
}

#[tauri::command]
pub async fn delete_server_full(
    opts: DeleteServerOptions,
    state: State<'_, AppState>,
) -> AppResult<DeleteServerReport> {
    let (server, assigned_key, dependents) = {
        let store = state.store.read().await;
        let server = store
            .find_server(opts.id)
            .ok_or(AppError::UnknownServer)?
            .clone();
        let key = match &server.auth {
            AuthMode::Key { key_id } => store.find_key(*key_id).cloned(),
            AuthMode::Password | AuthMode::Agent => None,
        };
        let dependents: Vec<String> = store
            .servers_jumping_via(server.id)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        (server, key, dependents)
    };
    if !dependents.is_empty() && !opts.force {
        return Err(AppError::Invalid(format!(
            "„{}“ ist Jump-Host für: {}. Diese Server zuerst umstellen.",
            server.name,
            dependents.join(", ")
        )));
    }

    let mut report = DeleteServerReport::default();

    if opts.cleanup_remote && !opts.force {
        if let Some(key) = &assigned_key {
            // Key login over the (possibly jump-routed) target; the password
            // variant below reuses the same route.
            let key_login = {
                let store = state.store.read().await;
                Login::new(&store, &state.secrets, &server, None)?
            };
            report.remote_cleanup_attempted = true;
            let password = opts
                .password
                .as_deref()
                .filter(|password| !password.is_empty());
            match password {
                // Key-only account: password login would not help, so leave the
                // hardening in place and only revoke if another key remains.
                None if opts.other_key_confirmed => {
                    report.hardening_kept = server.hardened;
                    let result =
                        revoke::revoke_keeping_other(&key_login, &key.public_openssh).await;
                    match result {
                        Ok(()) => report.remote_cleanup_succeeded = true,
                        Err(e @ (AppError::HostKey(_) | AppError::SecretRequired(_))) => {
                            return Err(e)
                        }
                        Err(e) => {
                            report.remote_cleanup_error = Some(e.to_string());
                            return Ok(report);
                        }
                    }
                }
                None => {
                    report.remote_cleanup_error = Some(
                        "Vor dem Entfernen des SSH-Keys wird ein funktionierender Passwort-Login \
                         oder ein anderer SSH-Key auf dem Server benötigt."
                            .into(),
                    );
                    return Ok(report);
                }
                Some(password) => {
                    // Password auth is off on hardened servers, so undo the hardening
                    // first. If the password then fails, restore the hardening.
                    let mut unhardened = false;
                    if server.hardened {
                        report.remote_unharden_attempted = true;
                        let unharden_result: AppResult<()> = async {
                            let session = key_login.connect().await?;
                            harden::unharden(session, opts.sudo_password.as_deref()).await
                        }
                        .await;
                        match unharden_result {
                            Ok(()) => {
                                report.remote_unharden_succeeded = true;
                                unhardened = true;
                            }
                            Err(e @ (AppError::HostKey(_) | AppError::SecretRequired(_))) => {
                                return Err(e)
                            }
                            Err(e) => {
                                report.remote_unharden_error = Some(e.to_string());
                                return Ok(report);
                            }
                        }
                    }

                    let password_login =
                        key_login.with_cred(Credential::Password(password.to_string()));
                    let result =
                        revoke::revoke_with_password(&password_login, &key.public_openssh).await;
                    match result {
                        Ok(()) => report.remote_cleanup_succeeded = true,
                        Err(e) => {
                            if unhardened {
                                match restore_hardening(&key_login, opts.sudo_password.as_deref())
                                    .await
                                {
                                    Ok(()) => report.hardening_restored = true,
                                    Err(restore_error) => {
                                        report.hardening_restore_error = Some(restore_error);
                                        mark_unhardened(&state, opts.id).await?;
                                    }
                                }
                            }
                            if let AppError::HostKey(_) = e {
                                return Err(e);
                            }
                            // A rejected cleanup password is reported, not re-prompted:
                            // the dialog owns that field.
                            let e = match e {
                                AppError::SecretRequired(_) => AppError::AuthFailed,
                                other => other,
                            };
                            report.remote_cleanup_error = Some(e.to_string());
                            return Ok(report);
                        }
                    }
                }
            }
        }
    }

    // Local delete.
    let mut store = state.store.write().await;
    let current_server = store.find_server(opts.id).ok_or(AppError::UnknownServer)?;
    let current_key = assigned_key.and_then(|key| match &current_server.auth {
        AuthMode::Key { key_id } if *key_id == key.id => store.find_key(*key_id).cloned(),
        _ => None,
    });
    store.servers.retain(|s| s.id != opts.id);
    store.clear_dangling_jump_refs();
    state.secrets.forget(SecretKind::ServerPassword, opts.id);
    report.server_removed = true;

    if let Some(key) = current_key {
        let still_referenced = store
            .servers
            .iter()
            .any(|s| matches!(&s.auth, AuthMode::Key { key_id } if *key_id == key.id));
        report.key_still_referenced = still_referenced;
        let remove_local_key = report.remote_cleanup_succeeded
            || (!opts.cleanup_remote && !opts.force && opts.delete_unused_local_key);
        if !still_referenced && remove_local_key {
            if let Err(e) = keygen::delete_key_files(&key) {
                report.local_key_error = Some(e.to_string());
            } else {
                store.keys.retain(|k| k.id != key.id);
                state.secrets.forget(SecretKind::KeyPassphrase, key.id);
                report.local_key_removed = true;
            }
        }
    }

    store.save(&state.store_file())?;

    if let Err(e) = ssh_config::sync(&store) {
        tracing::warn!("ssh_config sync after delete failed: {e}");
    }

    Ok(report)
}

#[tauri::command]
pub async fn delete_server(id: Uuid, state: State<'_, AppState>) -> AppResult<()> {
    let mut store = state.store.write().await;
    let before = store.servers.len();
    store.servers.retain(|s| s.id != id);
    if store.servers.len() == before {
        return Err(AppError::UnknownServer);
    }
    store.clear_dangling_jump_refs();
    state.secrets.forget(SecretKind::ServerPassword, id);
    store.save(&state.store_file())?;
    Ok(())
}
