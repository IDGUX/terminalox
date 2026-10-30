use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::prelude::*;
use chrono::{DateTime, Utc};
use rand::Rng;
use rand::rngs::SysRng;
use rand_core::UnwrapErr;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::keygen;
use crate::models::{Folder, KeyInfo, Server};
use crate::state::AppState;

pub const EXPORT_VERSION: u32 = 1;
pub const MAGIC: &str = "terminalox-backup";

#[derive(Debug, Serialize, Deserialize)]
struct KeyBundle {
    #[serde(flatten)]
    info: KeyInfo,
    /// Private key file contents, base64-encoded.
    private_b64: String,
    /// Public key file contents, base64-encoded.
    public_b64: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupPayload {
    magic: String,
    version: u32,
    exported_at: DateTime<Utc>,
    source_platform: String,
    servers: Vec<Server>,
    keys: Vec<KeyBundle>,
    /// Missing in backups from before folders existed.
    #[serde(default)]
    folders: Vec<Folder>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "format")]
enum BackupEnvelope {
    /// Plain JSON payload (no encryption).
    #[serde(rename = "plain")]
    Plain { payload: BackupPayload },
    /// AES-256-GCM with Argon2id KDF.
    #[serde(rename = "aes256gcm-argon2id")]
    Encrypted {
        salt_b64: String,
        nonce_b64: String,
        ciphertext_b64: String,
    },
}

fn argon2_params() -> Params {
    // Balanced: ~64 MiB, 3 iterations, 4 lanes. Tuned for desktop interactivity.
    Params::new(64 * 1024, 3, 4, Some(32)).expect("valid argon2 params")
}

fn derive_key(passphrase: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    let mut out = [0u8; 32];
    let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params());
    a2.hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| AppError::Other(format!("argon2 kdf failed: {e}")))?;
    Ok(out)
}

fn encrypt(passphrase: &str, payload: &BackupPayload) -> AppResult<BackupEnvelope> {
    let mut rng = UnwrapErr(SysRng);
    let mut salt = [0u8; 16];
    rng.fill_bytes(&mut salt);
    let mut nonce_bytes = [0u8; 12];
    rng.fill_bytes(&mut nonce_bytes);

    let key_bytes = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let plaintext = serde_json::to_vec(payload)?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_ref())
        .map_err(|e| AppError::Other(format!("aes-gcm encrypt: {e}")))?;

    Ok(BackupEnvelope::Encrypted {
        salt_b64: BASE64_STANDARD.encode(salt),
        nonce_b64: BASE64_STANDARD.encode(nonce_bytes),
        ciphertext_b64: BASE64_STANDARD.encode(ciphertext),
    })
}

fn decrypt(passphrase: &str, envelope: &BackupEnvelope) -> AppResult<BackupPayload> {
    match envelope {
        BackupEnvelope::Plain { payload } => Ok(clone_payload(payload)),
        BackupEnvelope::Encrypted {
            salt_b64,
            nonce_b64,
            ciphertext_b64,
        } => {
            let salt = BASE64_STANDARD
                .decode(salt_b64)
                .map_err(|e| AppError::Other(format!("b64 salt: {e}")))?;
            let nonce_bytes = BASE64_STANDARD
                .decode(nonce_b64)
                .map_err(|e| AppError::Other(format!("b64 nonce: {e}")))?;
            let ciphertext = BASE64_STANDARD
                .decode(ciphertext_b64)
                .map_err(|e| AppError::Other(format!("b64 ciphertext: {e}")))?;
            if nonce_bytes.len() != 12 {
                return Err(AppError::Other("invalid nonce length".into()));
            }
            let key_bytes = derive_key(passphrase, &salt)?;
            let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
            let plaintext = cipher
                .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
                .map_err(|_| AppError::Other("falsche Passphrase oder beschädigte Datei".into()))?;
            Ok(serde_json::from_slice(&plaintext)?)
        }
    }
}

fn clone_payload(p: &BackupPayload) -> BackupPayload {
    // BackupPayload doesn't impl Clone; round-trip via JSON.
    serde_json::to_value(p)
        .and_then(serde_json::from_value)
        .expect("payload json round-trip")
}

fn current_platform() -> String {
    std::env::consts::OS.to_string()
}

// ---------- Commands ----------

#[tauri::command]
pub async fn export_backup(
    target_path: String,
    passphrase: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<ExportReport> {
    let store = state.store.read().await;

    let mut bundles = Vec::with_capacity(store.keys.len());
    for key in &store.keys {
        let private_b64 = read_file_b64(Path::new(&key.private_path))?;
        let public_b64 = read_file_b64(Path::new(&key.public_path))?;
        bundles.push(KeyBundle {
            info: key.clone(),
            private_b64,
            public_b64,
        });
    }

    let payload = BackupPayload {
        magic: MAGIC.to_string(),
        version: EXPORT_VERSION,
        exported_at: Utc::now(),
        source_platform: current_platform(),
        servers: store.servers.clone(),
        keys: bundles,
        folders: store.folders.clone(),
    };

    let envelope = match passphrase.as_deref() {
        Some(pw) if !pw.is_empty() => encrypt(pw, &payload)?,
        _ => BackupEnvelope::Plain { payload },
    };

    let json = serde_json::to_vec_pretty(&envelope)?;
    let path = PathBuf::from(&target_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, json)?;

    let (n_servers, n_keys) = (store.servers.len(), store.keys.len());
    Ok(ExportReport {
        target_path,
        encrypted: matches!(envelope, BackupEnvelope::Encrypted { .. }),
        servers: n_servers,
        keys: n_keys,
    })
}

#[tauri::command]
pub async fn inspect_backup(
    source_path: String,
    passphrase: Option<String>,
) -> AppResult<InspectReport> {
    let bytes = std::fs::read(&source_path)?;
    let envelope: BackupEnvelope = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::Other(format!("Datei ist kein gültiges terminalox-Backup: {e}")))?;
    let encrypted = matches!(envelope, BackupEnvelope::Encrypted { .. });
    let payload = match (&envelope, passphrase.as_deref()) {
        (BackupEnvelope::Plain { payload }, _) => clone_payload(payload),
        (BackupEnvelope::Encrypted { .. }, Some(pw)) if !pw.is_empty() => decrypt(pw, &envelope)?,
        (BackupEnvelope::Encrypted { .. }, _) => {
            return Ok(InspectReport {
                encrypted,
                version: None,
                exported_at: None,
                source_platform: None,
                servers: 0,
                keys: 0,
                needs_passphrase: true,
                server_names: vec![],
                key_names: vec![],
            });
        }
    };
    if payload.magic != MAGIC {
        return Err(AppError::Other("Nicht unser Backup-Format".into()));
    }
    Ok(InspectReport {
        encrypted,
        version: Some(payload.version),
        exported_at: Some(payload.exported_at),
        source_platform: Some(payload.source_platform),
        servers: payload.servers.len(),
        keys: payload.keys.len(),
        needs_passphrase: false,
        server_names: payload.servers.iter().map(|s| s.name.clone()).collect(),
        key_names: payload.keys.iter().map(|k| k.info.name.clone()).collect(),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    pub source_path: String,
    pub passphrase: Option<String>,
    /// When true, servers+keys with matching IDs get replaced; otherwise they're skipped.
    #[serde(default)]
    pub replace_existing: bool,
}

#[tauri::command]
pub async fn import_backup(
    opts: ImportOptions,
    state: State<'_, AppState>,
) -> AppResult<ImportReport> {
    let bytes = std::fs::read(&opts.source_path)?;
    let envelope: BackupEnvelope = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::Other(format!("Datei ist kein gültiges terminalox-Backup: {e}")))?;
    let payload = match (&envelope, opts.passphrase.as_deref()) {
        (BackupEnvelope::Plain { payload }, _) => clone_payload(payload),
        (BackupEnvelope::Encrypted { .. }, Some(pw)) if !pw.is_empty() => decrypt(pw, &envelope)?,
        (BackupEnvelope::Encrypted { .. }, _) => {
            return Err(AppError::Other("Passphrase erforderlich".into()));
        }
    };
    if payload.magic != MAGIC {
        return Err(AppError::Other("Nicht unser Backup-Format".into()));
    }
    if payload.version > EXPORT_VERSION {
        return Err(AppError::Other(format!(
            "Backup-Version {} ist neuer als unterstützt ({}). Bitte App aktualisieren.",
            payload.version, EXPORT_VERSION
        )));
    }

    // Prepare writes (keys into ~/.ssh/ of *this* platform).
    let ssh_dir = keygen::ssh_dir()?;

    let mut report = ImportReport {
        servers_added: 0,
        servers_updated: 0,
        servers_skipped: 0,
        keys_added: 0,
        keys_updated: 0,
        keys_skipped: 0,
        key_file_conflicts: vec![],
    };

    let mut store = state.store.write().await;

    // ---- Keys first: write files, update KeyInfo paths to match this platform. ----
    for bundle in payload.keys {
        let existing = store
            .keys
            .iter()
            .position(|k| k.id == bundle.info.id || k.fingerprint == bundle.info.fingerprint);
        if existing.is_some() && !opts.replace_existing {
            report.keys_skipped += 1;
            continue;
        }

        let safe = sanitize_for_filename(&bundle.info.name);
        let prefix = if bundle.info.origin_imported {
            "terminalox_imported"
        } else {
            "terminalox"
        };
        let private_path = ssh_dir.join(format!("{prefix}_{safe}"));
        let public_path = ssh_dir.join(format!("{prefix}_{safe}.pub"));

        // If file paths are in use, generate unique variant.
        let (final_private, final_public) = pick_non_colliding(&private_path, &public_path);
        if final_private != private_path {
            report
                .key_file_conflicts
                .push(format!("{} → {}", private_path.display(), final_private.display()));
        }

        let private_bytes = BASE64_STANDARD
            .decode(&bundle.private_b64)
            .map_err(|e| AppError::Other(format!("b64 private: {e}")))?;
        let public_bytes = BASE64_STANDARD
            .decode(&bundle.public_b64)
            .map_err(|e| AppError::Other(format!("b64 public: {e}")))?;

        write_private(&final_private, &private_bytes)?;
        std::fs::write(&final_public, &public_bytes)?;

        let mut info = bundle.info.clone();
        info.private_path = final_private.to_string_lossy().to_string();
        info.public_path = final_public.to_string_lossy().to_string();

        if let Some(idx) = existing {
            store.keys[idx] = info;
            report.keys_updated += 1;
        } else {
            store.keys.push(info);
            report.keys_added += 1;
        }
    }

    // ---- Folders (before servers, so folder_id refs resolve). ----
    for folder in payload.folders {
        match store.folders.iter().position(|f| f.id == folder.id) {
            Some(idx) if opts.replace_existing => store.folders[idx] = folder,
            Some(_) => {}
            None => store.folders.push(folder),
        }
    }

    // ---- Servers. ----
    for server in payload.servers {
        let existing = store.servers.iter().position(|s| s.id == server.id);
        if existing.is_some() && !opts.replace_existing {
            report.servers_skipped += 1;
            continue;
        }
        if let Some(idx) = existing {
            store.servers[idx] = server;
            report.servers_updated += 1;
        } else {
            store.servers.push(server);
            report.servers_added += 1;
        }
    }

    store.clear_dangling_folder_refs();
    store.save(&state.store_file())?;
    Ok(report)
}

// ---------- Helpers ----------

fn read_file_b64(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(BASE64_STANDARD.encode(bytes))
}

fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn pick_non_colliding(private_path: &Path, public_path: &Path) -> (PathBuf, PathBuf) {
    if !private_path.exists() && !public_path.exists() {
        return (private_path.to_path_buf(), public_path.to_path_buf());
    }
    let parent = private_path.parent().unwrap_or(Path::new("."));
    let stem = private_path.file_name().unwrap_or_default().to_string_lossy();
    for n in 1..1000 {
        let candidate_priv = parent.join(format!("{stem}_imp{n}"));
        let candidate_pub = parent.join(format!("{stem}_imp{n}.pub"));
        if !candidate_priv.exists() && !candidate_pub.exists() {
            return (candidate_priv, candidate_pub);
        }
    }
    (private_path.to_path_buf(), public_path.to_path_buf())
}

fn write_private(path: &Path, bytes: &[u8]) -> AppResult<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

// ---------- Reports ----------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub target_path: String,
    pub encrypted: bool,
    pub servers: usize,
    pub keys: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectReport {
    pub encrypted: bool,
    pub version: Option<u32>,
    pub exported_at: Option<DateTime<Utc>>,
    pub source_platform: Option<String>,
    pub servers: usize,
    pub keys: usize,
    pub needs_passphrase: bool,
    pub server_names: Vec<String>,
    pub key_names: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub servers_added: usize,
    pub servers_updated: usize,
    pub servers_skipped: usize,
    pub keys_added: usize,
    pub keys_updated: usize,
    pub keys_skipped: usize,
    pub key_file_conflicts: Vec<String>,
}

