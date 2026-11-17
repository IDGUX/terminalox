//! Passwords and key passphrases.
//!
//! Secrets the user chose to remember go into the OS keychain (macOS Keychain,
//! Windows Credential Manager, Secret Service on Linux). Everything else lives
//! only in memory for the lifetime of the app, so a jump-host password typed
//! once is not asked again for every new tab.

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::error::{AppError, AppResult};

const SERVICE: &str = "dev.terminalox.app";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    /// SSH login password of a server.
    ServerPassword,
    /// Passphrase of an encrypted private key.
    KeyPassphrase,
}

impl SecretKind {
    fn prefix(self) -> &'static str {
        match self {
            SecretKind::ServerPassword => "password",
            SecretKind::KeyPassphrase => "passphrase",
        }
    }
}

fn account(kind: SecretKind, id: Uuid) -> String {
    format!("{}:{id}", kind.prefix())
}

#[derive(Default)]
pub struct Secrets {
    session: Mutex<HashMap<(SecretKind, Uuid), String>>,
}

impl Secrets {
    /// Memory first, then the OS keychain. Keychain failures (no Secret Service
    /// running, access denied) count as "not stored".
    pub fn get(&self, kind: SecretKind, id: Uuid) -> Option<String> {
        if let Some(v) = self.session.lock().unwrap().get(&(kind, id)) {
            return Some(v.clone());
        }
        let entry = keyring::Entry::new(SERVICE, &account(kind, id)).ok()?;
        match entry.get_password() {
            Ok(v) => {
                self.session.lock().unwrap().insert((kind, id), v.clone());
                Some(v)
            }
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                tracing::warn!("keychain read failed: {e}");
                None
            }
        }
    }

    /// Always caches in memory; with `remember` also persists in the keychain.
    pub fn set(&self, kind: SecretKind, id: Uuid, value: &str, remember: bool) -> AppResult<()> {
        self.session
            .lock()
            .unwrap()
            .insert((kind, id), value.to_string());
        if remember {
            keyring::Entry::new(SERVICE, &account(kind, id))
                .and_then(|e| e.set_password(value))
                .map_err(|e| {
                    AppError::from(msg!(
                        "keychain_save_failed",
                        "Saving to the keychain failed: {detail}",
                        detail = e
                    ))
                })?;
        }
        Ok(())
    }

    /// Removes the secret from memory and keychain.
    pub fn forget(&self, kind: SecretKind, id: Uuid) {
        self.session.lock().unwrap().remove(&(kind, id));
        if let Ok(entry) = keyring::Entry::new(SERVICE, &account(kind, id)) {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => tracing::warn!("keychain delete failed: {e}"),
            }
        }
    }

    /// True if the secret is persisted in the keychain (not just in memory).
    pub fn is_remembered(&self, kind: SecretKind, id: Uuid) -> bool {
        keyring::Entry::new(SERVICE, &account(kind, id))
            .and_then(|e| e.get_password())
            .is_ok()
    }
}
