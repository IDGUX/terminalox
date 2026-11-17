use chrono::Utc;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::access::Login;
use crate::error::{AppError, AppResult};
use crate::models::AuthMode;
use crate::ssh::harden::{self, HardenReadiness, VerifyOutcome};
use crate::state::AppState;

const PASSWORD_AUTH_HINT: &str =
    "Server ist noch auf Passwort-Auth — bitte erst einen Key deployen.";

/// Key/agent login for a server; `None` if it still uses password auth.
async fn key_login(state: &State<'_, AppState>, server_id: Uuid) -> AppResult<Option<Login>> {
    let store = state.store.read().await;
    let server = store
        .find_server(server_id)
        .ok_or(AppError::UnknownServer)?;
    if matches!(server.auth, AuthMode::Password) {
        return Ok(None);
    }
    Ok(Some(Login::new(&store, &state.secrets, server, None)?))
}

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
pub async fn check_harden_readiness(
    server_id: Uuid,
    state: State<'_, AppState>,
) -> AppResult<HardenReadiness> {
    let Some(login) = key_login(&state, server_id).await? else {
        return Ok(HardenReadiness::Unavailable {
            reason: PASSWORD_AUTH_HINT.into(),
        });
    };

    let session = match login.connect().await {
        Ok(s) => s,
        Err(e @ (AppError::HostKey(_) | AppError::SecretRequired(_))) => return Err(e),
        Err(e) => {
            return Ok(HardenReadiness::Unavailable {
                reason: format!("Key-Auth funktioniert nicht: {e}"),
            });
        }
    };
    harden::probe_readiness(session).await
}

#[tauri::command]
pub async fn harden_server(
    server_id: Uuid,
    sudo_password: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<HardenReport> {
    let login = key_login(&state, server_id)
        .await?
        .ok_or_else(|| AppError::Invalid(PASSWORD_AUTH_HINT.into()))?;

    let mut report = HardenReport::default();

    // Step 1: prove that the key actually grants access.
    let session = match login.connect().await {
        Ok(s) => {
            report.key_auth_verified = true;
            s
        }
        Err(e @ (AppError::HostKey(_) | AppError::SecretRequired(_))) => return Err(e),
        Err(e) => {
            report.warnings.push(format!(
                "Key-Auth-Check fehlgeschlagen: {e}. Härten wurde nicht versucht."
            ));
            return Ok(report);
        }
    };

    // Step 2: run the harden flow (drop-in + validate + reload + verify, with rollback).
    let outcome = harden::harden(session, &login, sudo_password.as_deref()).await?;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyHardenReport {
    pub dropin_present: bool,
    pub dropin_content_intact: bool,
    /// `Some(true)` = both password methods are disabled for this connection.
    /// `Some(false)` = at least one password method is enabled.
    /// `null` = couldn't determine (no sudo path).
    pub effective_password_auth_off: Option<bool>,
    pub effective_check_error: Option<String>,
    /// Final verdict the app uses.
    pub is_hardened: bool,
    /// `true` if we updated the persisted server flag as a result.
    pub state_changed: bool,
    pub previous_state: bool,
}

#[tauri::command]
pub async fn verify_server_hardening(
    server_id: Uuid,
    sudo_password: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<VerifyHardenReport> {
    let login = key_login(&state, server_id).await?.ok_or_else(|| {
        AppError::Invalid("Prüfung nur für Server mit Key- oder Agent-Login verfügbar.".into())
    })?;
    let was_hardened = login.server.hardened;

    let session = login.connect().await?;
    let outcome: VerifyOutcome = harden::verify(session, sudo_password.as_deref()).await?;

    let is_hardened = outcome.is_hardened();

    // Persist the new state if it differs.
    let mut state_changed = false;
    let can_update = !outcome.dropin_present
        || !outcome.dropin_content_intact
        || outcome.effective_password_auth_off.is_some();
    if can_update && is_hardened != was_hardened {
        let mut store = state.store.write().await;
        if let Some(s) = store.find_server_mut(server_id) {
            s.hardened = is_hardened;
            if is_hardened && s.hardened_at.is_none() {
                s.hardened_at = Some(Utc::now());
            }
            // Note: we deliberately keep hardened_at on drift-to-false so the
            // user can see when it WAS hardened. The hardened flag itself
            // reflects current reality.
            state_changed = true;
        }
        store.save(&state.store_file())?;
    }

    Ok(VerifyHardenReport {
        dropin_present: outcome.dropin_present,
        dropin_content_intact: outcome.dropin_content_intact,
        effective_password_auth_off: outcome.effective_password_auth_off,
        effective_check_error: outcome.effective_check_error,
        is_hardened,
        state_changed,
        previous_state: was_hardened,
    })
}
