use chrono::Utc;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::AuthMode;
use crate::ssh::harden;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HardenReport {
    pub key_auth_verified: bool,
    pub dropin_installed: bool,
    pub sshd_validated: bool,
    pub sshd_reloaded: bool,
    pub password_auth_confirmed_disabled: bool,
    pub hardened: bool,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn harden_server(
    server_id: Uuid,
    sudo_password: String,
    state: State<'_, AppState>,
) -> AppResult<HardenReport> {
    let (host, port, user, key_path) = {
        let store = state.store.read().await;
        let server = store
            .find_server(server_id)
            .ok_or(AppError::UnknownServer)?;
        let key_id = match &server.auth {
            AuthMode::Key { key_id } => *key_id,
            AuthMode::Password => {
                return Err(AppError::Other(
                    "Server ist noch auf Passwort-Auth — bitte erst einen Key deployen.".into(),
                ));
            }
        };
        let key = store.find_key(key_id).ok_or(AppError::UnknownKey)?;
        (
            server.host.clone(),
            server.port,
            server.user.clone(),
            key.private_path.clone(),
        )
    };

    let mut report = HardenReport::default();

    // Step 1: prove that the key actually grants access.
    let session = match harden::open_key_session(&host, port, &user, &key_path).await {
        Ok(s) => {
            report.key_auth_verified = true;
            s
        }
        Err(e) => {
            report.warnings.push(format!(
                "Key-Auth-Check fehlgeschlagen: {e}. Härten wurde nicht versucht."
            ));
            return Ok(report);
        }
    };

    // Step 2: run the harden flow (drop-in + validate + reload + verify, with rollback).
    let outcome = harden::harden(session, &sudo_password).await?;
    report.dropin_installed = outcome.dropin_installed;
    report.sshd_validated = outcome.sshd_validated;
    report.sshd_reloaded = outcome.sshd_reloaded;
    report.password_auth_confirmed_disabled = outcome.password_auth_confirmed_disabled;
    report.hardened = outcome.all_ok();
    if !outcome.all_ok() {
        let msg = if outcome.stderr.trim().is_empty() {
            "Härten fehlgeschlagen (kein Detail vom Server)".to_string()
        } else {
            format!("Härten fehlgeschlagen: {}", outcome.stderr.trim())
        };
        report.warnings.push(msg);
    }

    // Step 3: persist hardened flag.
    if report.hardened {
        let mut store = state.store.write().await;
        if let Some(s) = store.find_server_mut(server_id) {
            s.hardened = true;
            s.hardened_at = Some(Utc::now());
        }
        store.save(&state.store_file())?;
    }

    Ok(report)
}
