use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::keygen;
use crate::models::{AuthMode, NewServerInput, Server, UpdateServerInput};
use crate::ssh::{harden, revoke};
use crate::ssh_config;
use crate::state::AppState;

#[tauri::command]
pub async fn list_servers(state: State<'_, AppState>) -> AppResult<Vec<Server>> {
    let store = state.store.read().await;
    Ok(store.servers.clone())
}

#[tauri::command]
pub async fn add_server(
    input: NewServerInput,
    state: State<'_, AppState>,
) -> AppResult<Server> {
    let server = Server {
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
    };
    let mut store = state.store.write().await;
    if let Some(fid) = server.folder_id {
        store.find_folder(fid).ok_or(AppError::UnknownFolder)?;
    }
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
    let server = store
        .find_server_mut(input.id)
        .ok_or(AppError::UnknownServer)?;
    if let Some(v) = input.name {
        server.name = v;
    }
    if let Some(v) = input.host {
        server.host = v;
    }
    if let Some(v) = input.port {
        server.port = v;
    }
    if let Some(v) = input.user {
        server.user = v;
    }
    if let Some(v) = input.color {
        server.color = v;
    }
    if let Some(v) = input.auth {
        server.auth = v;
    }
    let updated = server.clone();
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
    /// Password for remote cleanup on password-auth servers.
    #[serde(default)]
    pub password: Option<String>,
    /// Sudo password used by unharden when the remote user needs one (Debian/Ubuntu default).
    /// Unused if the remote user is root or has NOPASSWD sudo.
    #[serde(default)]
    pub sudo_password: Option<String>,
    /// Proceed with local deletion even if remote cleanup fails.
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
    pub key_still_referenced: bool,
}

#[tauri::command]
pub async fn delete_server_full(
    opts: DeleteServerOptions,
    state: State<'_, AppState>,
) -> AppResult<DeleteServerReport> {
    let (server, assigned_key) = {
        let store = state.store.read().await;
        let server = store
            .find_server(opts.id)
            .ok_or(AppError::UnknownServer)?
            .clone();
        let key = match &server.auth {
            AuthMode::Key { key_id } => store.find_key(*key_id).cloned(),
            AuthMode::Password => None,
        };
        (server, key)
    };

    let mut report = DeleteServerReport::default();

    if opts.cleanup_remote {
        // Step A: if server is hardened, unharden first so password auth comes back.
        //         The unharden session uses the current key (we still have it).
        if server.hardened {
            report.remote_unharden_attempted = true;
            let unharden_result: AppResult<()> = (async {
                let key = assigned_key
                    .as_ref()
                    .ok_or_else(|| AppError::Other("hardened server has no key reference".into()))?;
                let session =
                    harden::open_key_session(&server.host, server.port, &server.user, &key.private_path)
                        .await?;
                // Unharden may need a sudo password (Debian/Ubuntu default).
                // Frontend typically passes the SSH password here as a sensible default.
                harden::unharden(session, opts.sudo_password.as_deref()).await
            })
            .await;

            match unharden_result {
                Ok(()) => report.remote_unharden_succeeded = true,
                Err(e @ AppError::HostKey(_)) => return Err(e),
                Err(e) => {
                    report.remote_unharden_error = Some(e.to_string());
                    if !opts.force {
                        return Ok(report);
                    }
                }
            }
        }

        // Step B: remove the deployed public key from authorized_keys.
        if let Some(key) = &assigned_key {
            report.remote_cleanup_attempted = true;
            let result = match &server.auth {
                AuthMode::Key { .. } => {
                    revoke::revoke_with_key(
                        &server.host,
                        server.port,
                        &server.user,
                        &key.private_path,
                        &key.public_openssh,
                    )
                    .await
                }
                AuthMode::Password => {
                    let pw = opts.password.as_deref().unwrap_or("");
                    if pw.is_empty() {
                        Err(AppError::Other(
                            "Passwort wird für Remote-Cleanup bei Passwort-Auth benötigt".into(),
                        ))
                    } else {
                        revoke::revoke_with_password(
                            &server.host,
                            server.port,
                            &server.user,
                            pw,
                            &key.public_openssh,
                        )
                        .await
                    }
                }
            };
            match result {
                Ok(()) => report.remote_cleanup_succeeded = true,
                Err(e @ AppError::HostKey(_)) => return Err(e),
                Err(e) => {
                    report.remote_cleanup_error = Some(e.to_string());
                    if !opts.force {
                        return Ok(report);
                    }
                }
            }
        }
    }

    // Local delete.
    let mut store = state.store.write().await;

    let before = store.servers.len();
    store.servers.retain(|s| s.id != opts.id);
    if store.servers.len() == before {
        return Err(AppError::UnknownServer);
    }
    report.server_removed = true;

    if let Some(key) = assigned_key {
        let still_referenced = store
            .servers
            .iter()
            .any(|s| matches!(&s.auth, AuthMode::Key { key_id } if *key_id == key.id));
        report.key_still_referenced = still_referenced;
        if !still_referenced {
            store.keys.retain(|k| k.id != key.id);
            if let Err(e) = keygen::delete_key_files(&key) {
                tracing::warn!("failed to delete key files: {e}");
            } else {
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
    store.save(&state.store_file())?;
    Ok(())
}
