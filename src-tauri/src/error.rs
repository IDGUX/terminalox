use serde::Serialize;
use thiserror::Error;

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

    #[error("unknown session id")]
    UnknownSession,

    #[error("authentication failed")]
    AuthFailed,

    #[error("remote command failed: {0}")]
    RemoteCommandFailed(String),

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
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
