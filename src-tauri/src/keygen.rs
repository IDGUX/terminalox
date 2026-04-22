use std::path::{Path, PathBuf};

use chrono::Utc;
use rand::rngs::SysRng;
use rand_core::UnwrapErr;
use russh::keys::ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::KeyInfo;

pub fn ssh_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| AppError::Other("home dir not found".into()))?;
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

pub fn generate_ed25519(name: &str) -> AppResult<KeyInfo> {
    let safe = sanitize(name);
    if safe.is_empty() {
        return Err(AppError::Other("key name must not be empty".into()));
    }
    let dir = ssh_dir()?;
    let private_path = dir.join(format!("terminalox_{safe}"));
    let public_path = dir.join(format!("terminalox_{safe}.pub"));
    if private_path.exists() {
        return Err(AppError::Other(format!(
            "key file already exists: {}",
            private_path.display()
        )));
    }

    let mut rng = UnwrapErr(SysRng);
    let mut key = PrivateKey::random(&mut rng, Algorithm::Ed25519)
        .map_err(|e| AppError::Other(format!("key generation failed: {e}")))?;
    let comment = format!("terminalox:{safe}");
    key.set_comment(&comment);

    let private_pem = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| AppError::Other(format!("encode private key: {e}")))?;
    let public_openssh = key
        .public_key()
        .to_openssh()
        .map_err(|e| AppError::Other(format!("encode public key: {e}")))?;
    let fingerprint = fingerprint_of(key.public_key());

    write_private(&private_path, private_pem.as_bytes())?;
    std::fs::write(&public_path, format!("{public_openssh}\n"))?;

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
    })
}

pub fn fingerprint_of(pubkey: &PublicKey) -> String {
    pubkey.fingerprint(HashAlg::Sha256).to_string()
}

/// Import an existing OpenSSH private key file. Copies it into the terminalox namespace
/// (`~/.ssh/terminalox_imported_<safe>`) so our delete flow can safely remove it without
/// touching the user's original file.
pub fn import_from_file(src_private_path: &Path, desired_name: &str) -> AppResult<KeyInfo> {
    let safe = sanitize(desired_name);
    if safe.is_empty() {
        return Err(AppError::Other("name must not be empty".into()));
    }
    if !src_private_path.exists() {
        return Err(AppError::Other(format!(
            "file not found: {}",
            src_private_path.display()
        )));
    }

    let bytes = std::fs::read(src_private_path)?;
    let content = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::Other("private key is not UTF-8 OpenSSH format".into()))?;

    let key = PrivateKey::from_openssh(content).map_err(|e| {
        AppError::Other(format!(
            "not a valid OpenSSH private key ({e}). If encrypted, decrypt first with `ssh-keygen -p -N '' -f <file>`."
        ))
    })?;
    if key.is_encrypted() {
        return Err(AppError::Other(
            "encrypted keys not yet supported; decrypt first with `ssh-keygen -p -N '' -f <file>`".into(),
        ));
    }

    let algorithm = match key.algorithm() {
        Algorithm::Ed25519 => "ed25519".to_string(),
        Algorithm::Rsa { .. } => "rsa".to_string(),
        Algorithm::Ecdsa { .. } => "ecdsa".to_string(),
        other => format!("{other:?}").to_lowercase(),
    };

    let dir = ssh_dir()?;
    let private_path = dir.join(format!("terminalox_imported_{safe}"));
    let public_path = dir.join(format!("terminalox_imported_{safe}.pub"));
    if private_path.exists() {
        return Err(AppError::Other(format!(
            "imported key with this name already exists: {}",
            private_path.display()
        )));
    }

    // Re-serialize to normalize format, then write with correct perms.
    let private_pem = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| AppError::Other(format!("encode private key: {e}")))?;
    let public_openssh = key
        .public_key()
        .to_openssh()
        .map_err(|e| AppError::Other(format!("encode public key: {e}")))?;
    let fingerprint = fingerprint_of(key.public_key());

    write_private(&private_path, private_pem.as_bytes())?;
    std::fs::write(&public_path, format!("{public_openssh}\n"))?;

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
    })
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

pub fn delete_key_files(info: &KeyInfo) -> AppResult<()> {
    for p in [&info.private_path, &info.public_path] {
        let path = Path::new(p);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}
