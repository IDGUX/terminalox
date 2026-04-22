use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::models::AuthMode;
use crate::storage::Store;

const BEGIN: &str = "# BEGIN TERMINALOX";
const END: &str = "# END TERMINALOX";

pub fn config_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| crate::error::AppError::Other("home dir not found".into()))?;
    Ok(home.join(".ssh").join("config"))
}

pub fn render_block(store: &Store) -> String {
    let mut lines = String::new();
    lines.push_str(BEGIN);
    lines.push_str(" — managed by terminalox, do not edit between markers\n");
    for server in &store.servers {
        // Match both the alias and the raw hostname/IP so that
        // `ssh <alias>` and `ssh <ip>` / `ssh user@<ip>` all pick up the key.
        let alias = alias_for(&server.name);
        if alias == server.host {
            lines.push_str(&format!("Host {}\n", alias));
        } else {
            lines.push_str(&format!("Host {} {}\n", alias, server.host));
        }
        lines.push_str(&format!("    HostName {}\n", server.host));
        lines.push_str(&format!("    User {}\n", server.user));
        if server.port != 22 {
            lines.push_str(&format!("    Port {}\n", server.port));
        }
        if let AuthMode::Key { key_id } = &server.auth {
            if let Some(key) = store.find_key(*key_id) {
                lines.push_str(&format!("    IdentityFile {}\n", key.private_path));
                lines.push_str("    IdentitiesOnly yes\n");
            }
        }
        lines.push('\n');
    }
    lines.push_str(END);
    lines.push('\n');
    lines
}

fn alias_for(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    safe
}

pub fn sync(store: &Store) -> AppResult<PathBuf> {
    let path = config_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let existing = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };

    let block = render_block(store);
    let updated = replace_block(&existing, &block);

    write_atomic(&path, updated.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }
    Ok(path)
}

fn replace_block(existing: &str, block: &str) -> String {
    if let (Some(start), Some(end_rel)) = (existing.find(BEGIN), existing[..].find(END)) {
        let end = end_rel
            + existing[end_rel..]
                .find('\n')
                .map(|n| n + 1)
                .unwrap_or(END.len());
        let mut out = String::with_capacity(existing.len());
        out.push_str(&existing[..start]);
        out.push_str(block);
        out.push_str(&existing[end..]);
        return out;
    }
    let mut out = String::with_capacity(existing.len() + block.len() + 2);
    out.push_str(existing);
    if !existing.is_empty() && !existing.ends_with('\n') {
        out.push('\n');
    }
    if !existing.is_empty() {
        out.push('\n');
    }
    out.push_str(block);
    out
}

fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
