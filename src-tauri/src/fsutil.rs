//! File writes that never expose secrets with default (umask) permissions.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::AppResult;

fn open_restricted(path: &Path, create_new: bool) -> std::io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true);
    if create_new {
        opts.create_new(true);
    } else {
        opts.create(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// Creates a new file readable only by the owner. Fails if the path exists, so
/// an existing key is never overwritten and no symlink is followed.
pub fn write_private_new(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let mut f = open_restricted(path, true)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

/// Atomically replaces `path` with `bytes`. The temporary file is created with
/// mode 0600 from the start, so the content is never readable by others.
pub fn write_atomic_private(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = tmp_path(path);
    let _ = std::fs::remove_file(&tmp);
    let result = (|| -> AppResult<()> {
        let mut f = open_restricted(&tmp, true)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".terminalox-tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("terminalox-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn private_new_refuses_to_overwrite() {
        let dir = temp_dir();
        let p = dir.join("key");
        write_private_new(&p, b"one").unwrap();
        assert!(write_private_new(&p, b"two").is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"one");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn atomic_replaces_and_restricts() {
        let dir = temp_dir();
        let p = dir.join("config");
        std::fs::write(&p, b"old").unwrap();
        write_atomic_private(&p, b"new").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"new");
        assert!(!tmp_path(&p).exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
