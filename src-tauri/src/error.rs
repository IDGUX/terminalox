use serde::Serialize;
use thiserror::Error;

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
    #[error("Dateizugriff: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("SSH: {0}")]
    Ssh(#[from] russh::Error),

    #[error("SSH-Key: {0}")]
    SshKey(#[from] russh::keys::Error),

    #[error("Server nicht gefunden")]
    UnknownServer,

    #[error("Key nicht gefunden")]
    UnknownKey,

    #[error("Ordner nicht gefunden")]
    UnknownFolder,

    #[error("Terminal-Sitzung nicht gefunden")]
    UnknownSession,

    #[error("Anmeldung fehlgeschlagen")]
    AuthFailed,

    #[error("Befehl auf dem Server fehlgeschlagen: {0}")]
    RemoteCommandFailed(String),

    #[error("{0}")]
    Invalid(String),

    /// Low-level: a key file needs a (different) passphrase. Commands turn this
    /// into `SecretRequired` with the key's id and name.
    #[error("Key ist verschlüsselt")]
    KeyPassphrase { wrong: bool },

    #[error("{}", secret_message(.0))]
    SecretRequired(SecretRequest),

    #[error("Host-Key von {}:{} nicht bestätigt — Verbindung abgebrochen", .0.host, .0.port)]
    HostKey(HostKeyRejection),

    #[error("{0}")]
    Other(String),
}

fn secret_message(r: &SecretRequest) -> String {
    match (r.kind, r.wrong) {
        ("passphrase", false) => format!("Passphrase für Key „{}“ erforderlich", r.label),
        ("passphrase", true) => format!("Passphrase für Key „{}“ ist falsch", r.label),
        (_, false) => format!("Passwort für „{}“ erforderlich", r.label),
        (_, true) => format!("Passwort für „{}“ wurde abgelehnt", r.label),
    }
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
        if let AppError::SecretRequired(r) = self {
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Structured<'a> {
                message: String,
                secret_required: &'a SecretRequest,
            }
            return Structured {
                message: self.to_string(),
                secret_required: r,
            }
            .serialize(s);
        }
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
