use std::path::PathBuf;

use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::keygen;
use crate::models::{AuthMode, KeyInfo, Server};
use crate::state::AppState;

#[tauri::command]
pub async fn list_keys(state: State<'_, AppState>) -> AppResult<Vec<KeyInfo>> {
    let store = state.store.read().await;
    Ok(store.keys.clone())
}

#[tauri::command]
pub async fn generate_key(name: String, state: State<'_, AppState>) -> AppResult<KeyInfo> {
    let info = keygen::generate_ed25519(&name)?;
    let mut store = state.store.write().await;
    store.keys.push(info.clone());
    store.save(&state.store_file())?;
    Ok(info)
}

#[tauri::command]
pub async fn import_key(
    source_path: String,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<KeyInfo> {
    let info = keygen::import_from_file(&PathBuf::from(source_path), &name)?;
    let mut store = state.store.write().await;
    // Prevent importing a duplicate by fingerprint.
    if let Some(fp) = &info.fingerprint {
        if store
            .keys
            .iter()
            .any(|k| k.fingerprint.as_deref() == Some(fp.as_str()))
        {
            // Clean up the file we just wrote before returning.
            let _ = keygen::delete_key_files(&info);
            return Err(AppError::Other(
                "dieser Key (Fingerprint) ist bereits importiert".into(),
            ));
        }
    }
    store.keys.push(info.clone());
    store.save(&state.store_file())?;
    Ok(info)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyUsage {
    pub key_id: Uuid,
    pub servers: Vec<Server>,
}

#[tauri::command]
pub async fn servers_using_key(
    key_id: Uuid,
    state: State<'_, AppState>,
) -> AppResult<KeyUsage> {
    let store = state.store.read().await;
    let servers: Vec<Server> = store
        .servers
        .iter()
        .filter(|s| matches!(&s.auth, AuthMode::Key { key_id: k } if *k == key_id))
        .cloned()
        .collect();
    Ok(KeyUsage { key_id, servers })
}

#[tauri::command]
pub async fn delete_key(id: Uuid, state: State<'_, AppState>) -> AppResult<()> {
    let mut store = state.store.write().await;
    // Refuse to delete if any server still references this key — prevents footguns.
    let referenced_by: Vec<String> = store
        .servers
        .iter()
        .filter(|s| matches!(&s.auth, AuthMode::Key { key_id } if *key_id == id))
        .map(|s| s.name.clone())
        .collect();
    if !referenced_by.is_empty() {
        return Err(AppError::Other(format!(
            "Key wird noch von {} Server(n) genutzt: {}. Zuerst diese Server löschen oder auf Passwort-Auth umstellen.",
            referenced_by.len(),
            referenced_by.join(", ")
        )));
    }

    let idx = store
        .keys
        .iter()
        .position(|k| k.id == id)
        .ok_or(AppError::UnknownKey)?;
    let info = store.keys.remove(idx);
    keygen::delete_key_files(&info)?;
    store.save(&state.store_file())?;
    Ok(())
}
