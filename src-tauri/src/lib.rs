mod commands;
mod error;
mod keygen;
mod models;
mod ssh;
mod ssh_config;
mod state;
mod storage;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;
use crate::storage::Store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,terminalox=debug,russh=warn")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir).ok();
            let store_path = data_dir.join("store.json");
            let store = Store::load(&store_path).unwrap_or_default();
            app.manage(AppState::new(data_dir, store));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::servers::list_servers,
            commands::servers::add_server,
            commands::servers::update_server,
            commands::servers::delete_server,
            commands::servers::delete_server_full,
            commands::folders::list_folders,
            commands::folders::create_folder,
            commands::folders::rename_folder,
            commands::folders::delete_folder,
            commands::folders::move_server,
            commands::keys::list_keys,
            commands::keys::generate_key,
            commands::keys::import_key,
            commands::keys::servers_using_key,
            commands::keys::delete_key,
            commands::session::open_session,
            commands::session::write_session,
            commands::session::resize_session,
            commands::session::close_session,
            commands::deploy::deploy_key,
            commands::harden::harden_server,
            commands::harden::check_harden_readiness,
            commands::harden::verify_server_hardening,
            commands::hostkeys::trust_host_key,
            commands::config::sync_ssh_config,
            commands::backup::export_backup,
            commands::backup::inspect_backup,
            commands::backup::import_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
