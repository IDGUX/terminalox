use base64::prelude::*;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;
use uuid::Uuid;

use crate::access::Login;
use crate::error::{AppError, AppResult};
use crate::ssh::session::{spawn_session, SessionClosedEvent, SessionLaunchParams, SessionOutput};
use crate::state::AppState;

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn open_session(
    server_id: Uuid,
    password: Option<String>,
    cols: u32,
    rows: u32,
    on_data: Channel<InvokeResponseBody>,
    on_closed: Channel<SessionClosedEvent>,
    state: State<'_, AppState>,
) -> AppResult<Uuid> {
    let login = {
        let store = state.store.read().await;
        let server = store
            .find_server(server_id)
            .ok_or(AppError::UnknownServer)?;
        Login::new(&store, &state.secrets, server, password)?
    };

    let session_id = spawn_session(
        SessionLaunchParams {
            login,
            cols: cols.max(1),
            rows: rows.max(1),
        },
        SessionOutput {
            data: on_data,
            closed: on_closed,
        },
        state.sessions.clone(),
    )
    .await?;

    {
        let mut store = state.store.write().await;
        if let Some(s) = store.find_server_mut(server_id) {
            s.last_used_at = Some(chrono::Utc::now());
        }
        if let Err(e) = store.save(&state.store_file()) {
            tracing::warn!("saving last_used_at failed: {e}");
        }
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
        .map_err(|e| AppError::Other(format!("Ungültige Eingabedaten: {e}")))?;
    // Clone the sender so the map lock is not held while waiting for queue space.
    let tx = {
        let sessions = state.sessions.read().await;
        sessions
            .get(&session_id)
            .ok_or(AppError::UnknownSession)?
            .input_tx
            .clone()
    };
    tx.send(bytes).await.map_err(|_| AppError::UnknownSession)?;
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
