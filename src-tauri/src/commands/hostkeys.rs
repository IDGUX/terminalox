use crate::error::AppResult;
use crate::ssh::client;

/// Called after the user confirmed the fingerprint from a `HostKey` error.
#[tauri::command]
pub async fn trust_host_key(host: String, port: u16, key_openssh: String) -> AppResult<()> {
    client::trust_host_key(&host, port, &key_openssh)
}
