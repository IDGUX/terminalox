use serde::Serialize;
use thiserror::Error;

use crate::msg::Msg;
use crate::ssh::client::HostKeyRejection;

/// A password or key passphrase the backend needs but does not have.
/// The frontend asks for it, stores it via `provide_secret` and retries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretRequest {
    /// "password" (server login) or "passphrase" (encrypted key).
    pub kind: &'static str,
    pub id: uuid::Uuid,
    /// Server or key name, for the prompt.
    pub label: String,
    /// true = a stored/typed value was tried and rejected.
    pub wrong: bool,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("File access: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("SSH: {0}")]
    Ssh(#[from] russh::Error),

    #[error("SSH key: {0}")]
    SshKey(#[from] russh::keys::Error),

    #[error("Server not found")]
    UnknownServer,

    #[error("Key not found")]
    UnknownKey,

    #[error("Folder not found")]
    UnknownFolder,

    #[error("Terminal session not found")]
    UnknownSession,

    #[error("Authentication failed")]
    AuthFailed,

    #[error("Command on the server failed: {0}")]
    RemoteCommandFailed(String),

    /// A translatable message (validation, refused operations …).
    #[error("{0}")]
    Msg(Msg),

    /// Low-level: a key file needs a (different) passphrase. Commands turn this
    /// into `SecretRequired` with the key's id and name.
    #[error("Key is encrypted")]
    KeyPassphrase { wrong: bool },

    #[error("{}", secret_msg(.0))]
    SecretRequired(SecretRequest),

    #[error("Host key of {}:{} not confirmed, connection aborted", .0.host, .0.port)]
    HostKey(HostKeyRejection),

    /// Technical detail without translation (library errors etc.).
    #[error("{0}")]
    Other(String),
}

fn secret_msg(r: &SecretRequest) -> Msg {
    let label = r.label.clone();
    match (r.kind, r.wrong) {
        ("passphrase", false) => msg!(
            "passphrase_required",
            "Passphrase for key \"{label}\" required",
            label = label
        ),
        ("passphrase", true) => msg!(
            "passphrase_wrong",
            "Passphrase for key \"{label}\" is wrong",
            label = label
        ),
        (_, false) => msg!(
            "password_required",
            "Password for \"{label}\" required",
            label = label
        ),
        (_, true) => msg!(
            "password_wrong",
            "Password for \"{label}\" was rejected",
            label = label
        ),
    }
}

impl From<Msg> for AppError {
    fn from(m: Msg) -> Self {
        AppError::Msg(m)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        AppError::Other(value.to_string())
    }
}

impl AppError {
    /// The error as translatable message (code + params + English text).
    pub fn to_msg(&self) -> Msg {
        match self {
            AppError::Io(e) => msg!("io", "File access: {detail}", detail = e),
            AppError::Json(e) => msg!("json", "JSON: {detail}", detail = e),
            AppError::Ssh(e) => msg!("ssh", "SSH: {detail}", detail = e),
            AppError::SshKey(e) => msg!("ssh_key", "SSH key: {detail}", detail = e),
            AppError::UnknownServer => msg!("unknown_server", "Server not found"),
            AppError::UnknownKey => msg!("unknown_key", "Key not found"),
            AppError::UnknownFolder => msg!("unknown_folder", "Folder not found"),
            AppError::UnknownSession => msg!("unknown_session", "Terminal session not found"),
            AppError::AuthFailed => msg!("auth_failed", "Authentication failed"),
            AppError::RemoteCommandFailed(d) => msg!(
                "remote_failed",
                "Command on the server failed: {detail}",
                detail = d
            ),
            AppError::Msg(m) => m.clone(),
            AppError::KeyPassphrase { .. } => msg!("key_encrypted", "Key is encrypted"),
            AppError::SecretRequired(r) => secret_msg(r),
            AppError::HostKey(r) => msg!(
                "host_key_rejected",
                "Host key of {host}:{port} not confirmed, connection aborted",
                host = r.host,
                port = r.port
            ),
            AppError::Other(d) => msg!("other", "{detail}", detail = d),
        }
    }
}

impl Serialize for AppError {
    /// Always an object: `{ code, params, text }` plus `hostKey` /
    /// `secretRequired` for errors the frontend resolves by asking the user.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Structured<'a> {
            #[serde(flatten)]
            msg: Msg,
            #[serde(skip_serializing_if = "Option::is_none")]
            host_key: Option<&'a HostKeyRejection>,
            #[serde(skip_serializing_if = "Option::is_none")]
            secret_required: Option<&'a SecretRequest>,
        }
        Structured {
            msg: self.to_msg(),
            host_key: match self {
                AppError::HostKey(r) => Some(r),
                _ => None,
            },
            secret_required: match self {
                AppError::SecretRequired(r) => Some(r),
                _ => None,
            },
        }
        .serialize(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;
