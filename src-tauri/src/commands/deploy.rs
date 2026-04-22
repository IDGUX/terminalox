use chrono::Utc;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::AuthMode;
use crate::ssh::harden::{HardenReadiness, PrivilegeMethod};
use crate::ssh::{deploy, harden};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeployReport {
    pub key_deployed: bool,
    pub key_auth_verified: bool,
    pub dropin_installed: bool,
    pub sshd_validated: bool,
    pub sshd_reloaded: bool,
    pub password_auth_confirmed_disabled: bool,
    pub hardened: bool,
    pub server_auth_switched: bool,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn deploy_key(
    server_id: Uuid,
    key_id: Uuid,
    password: String,
    // Optional override for sudo during hardening. If None, the SSH
    // password is reused as the sudo password when the remote user needs
    // one (typical Debian/Ubuntu case: SSH and sudo passwords are identical).
    sudo_password: Option<String>,
    assign_to_server: bool,
    state: State<'_, AppState>,
) -> AppResult<DeployReport> {
    let (host, port, user, public_openssh, key_path) = {
        let store = state.store.read().await;
        let server = store
            .find_server(server_id)
            .ok_or(AppError::UnknownServer)?;
        let key = store.find_key(key_id).ok_or(AppError::UnknownKey)?;
        (
            server.host.clone(),
            server.port,
            server.user.clone(),
            key.public_openssh.clone(),
            key.private_path.clone(),
        )
    };

    let mut report = DeployReport::default();

    // Step 1: Deploy key via password auth (appends to authorized_keys).
    deploy::deploy_public_key(&host, port, &user, &password, &public_openssh).await?;
    report.key_deployed = true;

    // Step 2: Pre-flight — fresh session using only the key proves it grants access.
    let key_session = match harden::open_key_session(&host, port, &user, &key_path).await {
        Ok(s) => {
            report.key_auth_verified = true;
            s
        }
        Err(e) => {
            report.warnings.push(format!(
                "Key-Pre-Flight fehlgeschlagen: {e}. Passwort-Login bleibt aktiv."
            ));
            return Ok(report);
        }
    };

    // Step 3: Probe privilege. Harden if possible — covers root, NOPASSWD,
    //         and the standard Debian/Ubuntu case (user in sudo group, needs
    //         a password). For the password case, we default to reusing the
    //         SSH password unless the UI provided an explicit override.
    let readiness = harden::probe_readiness(key_session).await?;
    match readiness {
        HardenReadiness::Ready { method } => {
            // Re-open session — probe_readiness consumed the first one.
            let harden_session =
                harden::open_key_session(&host, port, &user, &key_path).await?;
            let sudo_pw: Option<&str> = match method {
                PrivilegeMethod::Root | PrivilegeMethod::SudoNopasswd => None,
                PrivilegeMethod::SudoWithPassword => {
                    Some(sudo_password.as_deref().unwrap_or(password.as_str()))
                }
            };
            let outcome = harden::harden(harden_session, sudo_pw).await?;
            report.dropin_installed = outcome.dropin_installed;
            report.sshd_validated = outcome.sshd_validated;
            report.sshd_reloaded = outcome.sshd_reloaded;
            report.password_auth_confirmed_disabled = outcome.password_auth_confirmed_disabled;
            report.hardened = outcome.all_ok();
            if !outcome.all_ok() {
                let msg = if outcome.stderr.is_empty() {
                    "Härten fehlgeschlagen (kein Detail vom Server)".to_string()
                } else {
                    format!("Härten fehlgeschlagen: {}", outcome.stderr.trim())
                };
                report.warnings.push(msg);
            }
        }
        HardenReadiness::Unavailable { reason } => {
            report.warnings.push(format!(
                "Key ist deployed, aber Härten wurde übersprungen: {reason}"
            ));
        }
    }

    // Step 4: Update server state in the store.
    {
        let mut store = state.store.write().await;
        if let Some(s) = store.find_server_mut(server_id) {
            if assign_to_server {
                s.auth = AuthMode::Key { key_id };
                report.server_auth_switched = true;
            }
            if report.hardened {
                s.hardened = true;
                s.hardened_at = Some(Utc::now());
            }
        }
        store.save(&state.store_file())?;
    }

    Ok(report)
}
