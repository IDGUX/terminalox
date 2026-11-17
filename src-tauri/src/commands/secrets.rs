use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::access;
use crate::error::{AppError, AppResult};
use crate::secrets::SecretKind;
use crate::state::AppState;

fn kind_of(kind: &str) -> AppResult<SecretKind> {
    match kind {
        "password" => Ok(SecretKind::ServerPassword),
        "passphrase" => Ok(SecretKind::KeyPassphrase),
        other => Err(AppError::Invalid(format!("unbekannte Secret-Art: {other}"))),
    }
}

/// Stores a password/passphrase the backend asked for via `SecretRequired`.
/// Passphrases are checked against the key file first.
#[tauri::command]
pub async fn provide_secret(
    kind: String,
    id: Uuid,
    value: String,
    remember: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let kind = kind_of(&kind)?;
    {
        let store = state.store.read().await;
        match kind {
            SecretKind::KeyPassphrase => {
                let key = store.find_key(id).ok_or(AppError::UnknownKey)?;
                access::check_passphrase(key, &value)?;
            }
            SecretKind::ServerPassword => {
                store.find_server(id).ok_or(AppError::UnknownServer)?;
            }
        }
    }
    // Replace any earlier (possibly wrong) remembered value.
    state.secrets.forget(kind, id);
    state.secrets.set(kind, id, &value, remember)
}

#[tauri::command]
pub async fn forget_secret(kind: String, id: Uuid, state: State<'_, AppState>) -> AppResult<()> {
    state.secrets.forget(kind_of(&kind)?, id);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStatus {
    pub remembered: bool,
}

#[tauri::command]
pub async fn secret_status(
    kind: String,
    id: Uuid,
    state: State<'_, AppState>,
) -> AppResult<SecretStatus> {
    let kind = kind_of(&kind)?;
    Ok(SecretStatus {
        remembered: state.secrets.is_remembered(kind, id),
    })
}
