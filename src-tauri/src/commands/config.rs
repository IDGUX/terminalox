use tauri::State;

use crate::error::AppResult;
use crate::ssh_config;
use crate::state::AppState;

#[tauri::command]
pub async fn sync_ssh_config(state: State<'_, AppState>) -> AppResult<String> {
    let store = state.store.read().await;
    let path = ssh_config::sync(&store)?;
    Ok(path.to_string_lossy().to_string())
}
