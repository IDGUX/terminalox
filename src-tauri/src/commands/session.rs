use base64::prelude::*;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::AuthMode;
use crate::ssh::session::{spawn_session, SessionLaunchParams};
use crate::state::AppState;

#[tauri::command]
pub async fn open_session(
    server_id: Uuid,
    password: Option<String>,
    cols: u32,
    rows: u32,
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Uuid> {
    let (server, key) = {
        let store = state.store.read().await;
        let server = store
            .find_server(server_id)
            .ok_or(AppError::UnknownServer)?
            .clone();
        let key = match &server.auth {
            AuthMode::Key { key_id } => Some(
                store
                    .find_key(*key_id)
                    .ok_or(AppError::UnknownKey)?
                    .clone(),
            ),
            AuthMode::Password => None,
        };
        (server, key)
    };

    let (session_id, handle) = spawn_session(
        app,
        SessionLaunchParams {
            server,
            key,
            password,
            cols: cols.max(1),
            rows: rows.max(1),
        },
    )
    .await?;

    state.sessions.write().await.insert(session_id, handle);

    {
        let mut store = state.store.write().await;
        if let Some(s) = store.find_server_mut(server_id) {
            s.last_used_at = Some(chrono::Utc::now());
        }
        let _ = store.save(&state.store_file());
    }

    Ok(session_id)
}

#[tauri::command]
pub async fn write_session(
    session_id: Uuid,
    data_b64: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let bytes = BASE64_STANDARD
        .decode(data_b64.as_bytes())
        .map_err(|e| AppError::Other(format!("base64: {e}")))?;
    let sessions = state.sessions.read().await;
    let handle = sessions.get(&session_id).ok_or(AppError::UnknownSession)?;
    handle
        .input_tx
        .send(bytes)
        .map_err(|_| AppError::UnknownSession)?;
    Ok(())
}

#[tauri::command]
pub async fn resize_session(
    session_id: Uuid,
    cols: u32,
    rows: u32,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let sessions = state.sessions.read().await;
    let handle = sessions.get(&session_id).ok_or(AppError::UnknownSession)?;
    let _ = handle.resize_tx.send((cols.max(1), rows.max(1)));
    Ok(())
}

#[tauri::command]
pub async fn close_session(session_id: Uuid, state: State<'_, AppState>) -> AppResult<()> {
    let mut sessions = state.sessions.write().await;
    if let Some(mut handle) = sessions.remove(&session_id) {
        if let Some(cancel) = handle.cancel.take() {
            let _ = cancel.send(());
        }
    }
    Ok(())
}
