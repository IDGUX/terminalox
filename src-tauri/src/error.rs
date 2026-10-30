use serde::Serialize;
use thiserror::Error;

use crate::ssh::client::HostKeyRejection;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("ssh: {0}")]
    Ssh(#[from] russh::Error),

    #[error("ssh key: {0}")]
    SshKey(#[from] russh::keys::Error),

    #[error("unknown server id")]
    UnknownServer,

    #[error("unknown key id")]
    UnknownKey,

    #[error("unknown folder id")]
    UnknownFolder,

    #[error("unknown session id")]
    UnknownSession,

    #[error("authentication failed")]
    AuthFailed,

    #[error("remote command failed: {0}")]
    RemoteCommandFailed(String),

    #[error("Host-Key von {}:{} nicht bestätigt — Verbindung abgebrochen", .0.host, .0.port)]
    HostKey(HostKeyRejection),

    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        AppError::Other(value.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Host-key errors go out structured so the frontend can show a trust prompt;
        // everything else stays a plain string.
        if let AppError::HostKey(r) = self {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Structured<'a> {
                message: String,
                host_key: &'a HostKeyRejection,
            }
            return Structured {
                message: self.to_string(),
                host_key: r,
            }
            .serialize(s);
        }
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
