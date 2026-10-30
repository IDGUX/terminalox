use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{Folder, Server};
use crate::state::AppState;
use crate::storage::Store;

fn validate_name(store: &Store, name: &str, except: Option<Uuid>) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Other("Ordnername darf nicht leer sein".into()));
    }
    let taken = store
        .folders
        .iter()
        .any(|f| Some(f.id) != except && f.name.to_lowercase() == name.to_lowercase());
    if taken {
        return Err(AppError::Other(format!("Ordner „{name}\" existiert bereits")));
    }
    Ok(name.to_string())
}

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> AppResult<Vec<Folder>> {
    let store = state.store.read().await;
    Ok(store.folders.clone())
}

#[tauri::command]
pub async fn create_folder(name: String, state: State<'_, AppState>) -> AppResult<Folder> {
    let mut store = state.store.write().await;
    let folder = Folder {
        id: Uuid::new_v4(),
        name: validate_name(&store, &name, None)?,
        created_at: Utc::now(),
    };
    store.folders.push(folder.clone());
    store.save(&state.store_file())?;
    Ok(folder)
}

#[tauri::command]
pub async fn rename_folder(id: Uuid, name: String, state: State<'_, AppState>) -> AppResult<Folder> {
    let mut store = state.store.write().await;
    let name = validate_name(&store, &name, Some(id))?;
    let folder = store
        .folders
        .iter_mut()
        .find(|f| f.id == id)
        .ok_or(AppError::UnknownFolder)?;
    folder.name = name;
    let updated = folder.clone();
    store.save(&state.store_file())?;
    Ok(updated)
}

/// Deletes only the folder; its servers move back to "Ohne Ordner".
#[tauri::command]
pub async fn delete_folder(id: Uuid, state: State<'_, AppState>) -> AppResult<()> {
    let mut store = state.store.write().await;
    let before = store.folders.len();
    store.folders.retain(|f| f.id != id);
    if store.folders.len() == before {
        return Err(AppError::UnknownFolder);
    }
    store.clear_dangling_folder_refs();
    store.save(&state.store_file())?;
    Ok(())
}

#[tauri::command]
pub async fn move_server(
    server_id: Uuid,
    folder_id: Option<Uuid>,
    state: State<'_, AppState>,
) -> AppResult<Server> {
    let mut store = state.store.write().await;
    if let Some(fid) = folder_id {
        store.find_folder(fid).ok_or(AppError::UnknownFolder)?;
    }
    let server = store
        .find_server_mut(server_id)
        .ok_or(AppError::UnknownServer)?;
    server.folder_id = folder_id;
    let updated = server.clone();
    store.save(&state.store_file())?;
    Ok(updated)
}
