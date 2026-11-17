use std::path::{Path, PathBuf};

use chrono::Utc;
use rand::rngs::SysRng;
use rand_core::UnwrapErr;
use russh::keys::ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::fsutil;
use crate::models::KeyInfo;

pub fn ssh_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::Other("Home-Verzeichnis nicht gefunden".into()))?;
    let dir = home.join(".ssh");
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dir)?.permissions();
        if perms.mode() & 0o777 != 0o700 {
            perms.set_mode(0o700);
            std::fs::set_permissions(&dir, perms)?;
        }
    }
    Ok(dir)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Generates an Ed25519 key in `~/.ssh/terminalox_<name>`. With a passphrase,
/// the file is encrypted in the standard OpenSSH format.
pub fn generate_ed25519(name: &str, passphrase: Option<&str>) -> AppResult<KeyInfo> {
    let safe = sanitize(name);
    if safe.is_empty() {
        return Err(AppError::Invalid("Key-Name darf nicht leer sein".into()));
    }
    let dir = ssh_dir()?;
    let private_path = dir.join(format!("terminalox_{safe}"));
    let public_path = dir.join(format!("terminalox_{safe}.pub"));
    if private_path.exists() || public_path.exists() {
        return Err(AppError::Invalid(format!(
            "Key-Datei existiert bereits: {}",
            private_path.display()
        )));
    }

    let mut rng = UnwrapErr(SysRng);
    let mut key = PrivateKey::random(&mut rng, Algorithm::Ed25519)
        .map_err(|e| AppError::Other(format!("Key-Erzeugung fehlgeschlagen: {e}")))?;
    let comment = format!("terminalox:{safe}");
    key.set_comment(comment.as_str());

    let public_openssh = key
        .public_key()
        .to_openssh()
        .map_err(|e| AppError::Other(format!("Public Key kodieren: {e}")))?;
    let fingerprint = fingerprint_of(key.public_key());

    let passphrase = passphrase.filter(|p| !p.is_empty());
    let stored = match passphrase {
        Some(p) => key
            .encrypt(&mut rng, p)
            .map_err(|e| AppError::Other(format!("Key verschlüsseln: {e}")))?,
        None => key,
    };
    let private_pem = stored
        .to_openssh(LineEnding::LF)
        .map_err(|e| AppError::Other(format!("Private Key kodieren: {e}")))?;

    write_key_pair(
        &private_path,
        private_pem.as_bytes(),
        &public_path,
        &public_openssh,
    )?;

    Ok(KeyInfo {
        id: Uuid::new_v4(),
        name: safe,
        algorithm: "ed25519".to_string(),
        public_openssh,
        private_path: private_path.to_string_lossy().to_string(),
        public_path: public_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
        fingerprint: Some(fingerprint),
        origin_imported: false,
        encrypted: passphrase.is_some(),
    })
}

pub fn fingerprint_of(pubkey: &PublicKey) -> String {
    pubkey.fingerprint(HashAlg::Sha256).to_string()
}

/// Import an existing OpenSSH private key file. Copies it into the terminalox namespace
/// (`~/.ssh/terminalox_imported_<safe>`) so our delete flow can safely remove it without
/// touching the user's original file.
///
/// Encrypted keys are copied byte for byte (the passphrase is asked on first use);
/// unencrypted ones are re-serialised to normalise the format.
pub fn import_from_file(src_private_path: &Path, desired_name: &str) -> AppResult<KeyInfo> {
    let safe = sanitize(desired_name);
    if safe.is_empty() {
        return Err(AppError::Invalid("Name darf nicht leer sein".into()));
    }
    if !src_private_path.exists() {
        return Err(AppError::Invalid(format!(
            "Datei nicht gefunden: {}",
            src_private_path.display()
        )));
    }

    let bytes = std::fs::read(src_private_path)?;
    let content = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::Invalid("Private Key ist nicht im OpenSSH-Textformat".into()))?;

    let key = PrivateKey::from_openssh(content).map_err(|e| {
        AppError::Invalid(format!(
            "Kein gültiger OpenSSH-Private-Key ({e}). Ältere PEM-Keys lassen sich mit \
             `ssh-keygen -p -f <datei>` ins OpenSSH-Format umwandeln."
        ))
    })?;
    let encrypted = key.is_encrypted();

    let algorithm = match key.algorithm() {
        Algorithm::Ed25519 => "ed25519".to_string(),
        Algorithm::Rsa { .. } => "rsa".to_string(),
        Algorithm::Ecdsa { .. } => "ecdsa".to_string(),
        other => format!("{other:?}").to_lowercase(),
    };

    let dir = ssh_dir()?;
    let private_path = dir.join(format!("terminalox_imported_{safe}"));
    let public_path = dir.join(format!("terminalox_imported_{safe}.pub"));
    if private_path.exists() || public_path.exists() {
        return Err(AppError::Invalid(format!(
            "Ein importierter Key mit diesem Namen existiert bereits: {}",
            private_path.display()
        )));
    }

    let private_pem = if encrypted {
        content.to_string()
    } else {
        key.to_openssh(LineEnding::LF)
            .map_err(|e| AppError::Other(format!("Private Key kodieren: {e}")))?
            .to_string()
    };
    let public_openssh = key
        .public_key()
        .to_openssh()
        .map_err(|e| AppError::Other(format!("Public Key kodieren: {e}")))?;
    let fingerprint = fingerprint_of(key.public_key());

    write_key_pair(
        &private_path,
        private_pem.as_bytes(),
        &public_path,
        &public_openssh,
    )?;

    Ok(KeyInfo {
        id: Uuid::new_v4(),
        name: safe,
        algorithm,
        public_openssh,
        private_path: private_path.to_string_lossy().to_string(),
        public_path: public_path.to_string_lossy().to_string(),
        created_at: Utc::now(),
        fingerprint: Some(fingerprint),
        origin_imported: true,
        encrypted,
    })
}

/// Writes both files with owner-only permissions; never overwrites. If the
/// public file fails, the private one is removed again.
fn write_key_pair(
    private_path: &Path,
    private_bytes: &[u8],
    public_path: &Path,
    public_openssh: &str,
) -> AppResult<()> {
    fsutil::write_private_new(private_path, private_bytes)?;
    if let Err(e) = fsutil::write_private_new(public_path, format!("{public_openssh}\n").as_bytes())
    {
        let _ = std::fs::remove_file(private_path);
        return Err(e);
    }
    Ok(())
}

pub fn delete_key_files(info: &KeyInfo) -> AppResult<()> {
    for p in [&info.private_path, &info.public_path] {
        let path = Path::new(p);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}
