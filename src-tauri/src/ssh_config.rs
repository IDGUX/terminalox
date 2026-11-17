use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::fsutil;
use crate::models::{AuthMode, Server};
use crate::storage::Store;
use crate::validate;

const BEGIN: &str = "# BEGIN TERMINALOX";
const END: &str = "# END TERMINALOX";

pub fn config_path() -> AppResult<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| AppError::Other("home directory not found".into()))?;
    Ok(home.join(".ssh").join("config"))
}

/// `Host` alias per server: sanitised name, made unique with `-2`, `-3` …
fn aliases(servers: &[Server]) -> HashMap<Uuid, String> {
    let mut used = HashSet::new();
    let mut out = HashMap::new();
    for s in servers {
        let base = alias_for(&s.name);
        let mut alias = base.clone();
        let mut n = 2;
        while !used.insert(alias.to_lowercase()) {
            alias = format!("{base}-{n}");
            n += 1;
        }
        out.insert(s.id, alias);
    }
    out
}

/// Entries that would break the config (e.g. from an old store or a foreign
/// backup) are left out instead of being written.
fn is_safe(s: &Server) -> bool {
    validate::host(&s.host).is_ok_and(|h| h == s.host)
        && validate::user(&s.user).is_ok_and(|u| u == s.user)
}

pub fn render_block(store: &Store) -> String {
    let servers: Vec<Server> = store
        .servers
        .iter()
        .filter(|s| is_safe(s))
        .cloned()
        .collect();
    let aliases = aliases(&servers);

    // The raw host goes on the `Host` line only if exactly one entry uses it;
    // otherwise `ssh <ip>` would silently pick the first entry's user and key.
    let mut host_count: HashMap<String, usize> = HashMap::new();
    for s in &servers {
        *host_count.entry(s.host.to_lowercase()).or_default() += 1;
    }

    let mut lines = String::new();
    lines.push_str(BEGIN);
    lines.push_str(" — managed by terminalox, do not edit between markers\n");
    for server in &servers {
        let alias = &aliases[&server.id];
        let host_unique = host_count[&server.host.to_lowercase()] == 1;
        if alias.eq_ignore_ascii_case(&server.host) || !host_unique {
            lines.push_str(&format!("Host {alias}\n"));
        } else {
            lines.push_str(&format!("Host {alias} {}\n", server.host));
        }
        lines.push_str(&format!("    HostName {}\n", server.host));
        lines.push_str(&format!("    User {}\n", server.user));
        if server.port != 22 {
            lines.push_str(&format!("    Port {}\n", server.port));
        }
        if let AuthMode::Key { key_id } = &server.auth {
            if let Some(key) = store.find_key(*key_id) {
                lines.push_str(&format!("    IdentityFile \"{}\"\n", key.private_path));
                lines.push_str("    IdentitiesOnly yes\n");
            }
        }
        if let Some(jump) = server.jump_server_id.and_then(|id| aliases.get(&id)) {
            lines.push_str(&format!("    ProxyJump {jump}\n"));
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
    let trimmed = safe.trim_start_matches('-');
    if trimmed.is_empty() {
        "server".to_string()
    } else {
        trimmed.to_string()
    }
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
    if updated != existing {
        fsutil::write_atomic_private(&path, updated.as_bytes())?;
    }
    Ok(path)
}

/// Re-syncs only if the user already let terminalox manage a block, so
/// changes like a rotated key do not leave a stale `IdentityFile` behind.
pub fn sync_if_managed(store: &Store) -> AppResult<()> {
    let path = config_path()?;
    if std::fs::read_to_string(&path).is_ok_and(|c| c.contains(BEGIN)) {
        sync(store)?;
    }
    Ok(())
}

fn replace_block(existing: &str, block: &str) -> String {
    if let Some(start) = existing.find(BEGIN) {
        // END is searched only after BEGIN; a stray END earlier in the file
        // must not cut out user content.
        if let Some(end_rel) = existing[start..].find(END) {
            let end_abs = start + end_rel;
            let end = end_abs
                + existing[end_abs..]
                    .find('\n')
                    .map(|n| n + 1)
                    .unwrap_or(existing.len() - end_abs);
            let mut out = String::with_capacity(existing.len());
            out.push_str(&existing[..start]);
            out.push_str(block);
            out.push_str(&existing[end..]);
            return out;
        }
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

// ---------- Import from an existing ~/.ssh/config ----------

/// One concrete `Host` entry found in the user's config.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedHost {
    pub alias: String,
    pub host: String,
    pub user: Option<String>,
    pub port: u16,
    pub identity_file: Option<String>,
    /// First hop of `ProxyJump` as written (alias or `user@host:port`).
    pub proxy_jump: Option<String>,
}

#[derive(Default, Clone)]
struct Block {
    patterns: Vec<String>,
    host_name: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
    proxy_jump: Option<String>,
}

fn unquote(v: &str) -> String {
    v.trim().trim_matches('"').to_string()
}

fn expand_home(v: &str) -> String {
    match (v.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().to_string(),
        _ => v.to_string(),
    }
}

/// Parses `Host` blocks. Wildcard-only blocks provide defaults (like `Host *`),
/// `Match` blocks and terminalox's own managed block are ignored.
pub fn parse_hosts(text: &str) -> Vec<ParsedHost> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut cur: Option<Block> = None;
    let mut in_managed = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with(BEGIN) {
            in_managed = true;
            continue;
        }
        if line.starts_with(END) {
            in_managed = false;
            continue;
        }
        if in_managed || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (k.to_lowercase(), v.trim().trim_start_matches('=').trim()),
            None => continue,
        };
        match key.as_str() {
            "host" => {
                blocks.extend(cur.take());
                cur = Some(Block {
                    patterns: value.split_whitespace().map(unquote).collect(),
                    ..Default::default()
                });
            }
            "match" => {
                blocks.extend(cur.take());
                cur = None;
            }
            _ => {
                let Some(b) = cur.as_mut() else { continue };
                // OpenSSH: the first value obtained wins.
                match key.as_str() {
                    "hostname" if b.host_name.is_none() => b.host_name = Some(unquote(value)),
                    "user" if b.user.is_none() => b.user = Some(unquote(value)),
                    "port" if b.port.is_none() => b.port = value.parse().ok(),
                    "identityfile" if b.identity_file.is_none() => {
                        b.identity_file = Some(expand_home(&unquote(value)))
                    }
                    "proxyjump" if b.proxy_jump.is_none() => {
                        let first = value.split(',').next().unwrap_or("").trim();
                        if !first.is_empty() && !first.eq_ignore_ascii_case("none") {
                            b.proxy_jump = Some(first.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    blocks.extend(cur);

    let is_wild = |p: &str| p.contains(['*', '?', '!']);
    let defaults: Vec<&Block> = blocks
        .iter()
        .filter(|b| b.patterns.iter().all(|p| is_wild(p)))
        .collect();
    let default_of = |f: fn(&Block) -> Option<String>| defaults.iter().find_map(|b| f(b));

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for b in &blocks {
        let Some(alias) = b.patterns.iter().find(|p| !is_wild(p)) else {
            continue;
        };
        if !seen.insert(alias.to_lowercase()) {
            continue;
        }
        out.push(ParsedHost {
            alias: alias.clone(),
            host: b.host_name.clone().unwrap_or_else(|| alias.clone()),
            user: b.user.clone().or_else(|| default_of(|d| d.user.clone())),
            port: b
                .port
                .or_else(|| defaults.iter().find_map(|d| d.port))
                .unwrap_or(22),
            identity_file: b
                .identity_file
                .clone()
                .or_else(|| default_of(|d| d.identity_file.clone())),
            proxy_jump: b.proxy_jump.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::KeyInfo;
    use chrono::Utc;

    fn server(name: &str, host: &str) -> Server {
        Server {
            id: Uuid::new_v4(),
            name: name.into(),
            host: host.into(),
            port: 22,
            user: "root".into(),
            auth: AuthMode::Password,
            color: None,
            created_at: Utc::now(),
            last_used_at: None,
            hardened: false,
            hardened_at: None,
            folder_id: None,
            jump_server_id: None,
        }
    }

    #[test]
    fn replace_block_keeps_user_content_and_ignores_stray_end() {
        let existing = "# END TERMINALOX stray\nHost a\n\n# BEGIN TERMINALOX old\nHost x\n# END TERMINALOX\nHost b\n";
        let out = replace_block(existing, "# BEGIN TERMINALOX\nNEW\n# END TERMINALOX\n");
        assert_eq!(
            out,
            "# END TERMINALOX stray\nHost a\n\n# BEGIN TERMINALOX\nNEW\n# END TERMINALOX\nHost b\n"
        );
    }

    #[test]
    fn replace_block_appends_when_missing() {
        let out = replace_block("Host a", "BLOCK\n");
        assert_eq!(out, "Host a\n\nBLOCK\n");
        assert_eq!(replace_block("", "BLOCK\n"), "BLOCK\n");
    }

    #[test]
    fn render_skips_unsafe_entries_and_dedups_aliases() {
        let mut store = Store::default();
        store.servers.push(server("web", "10.0.0.1"));
        store.servers.push(server("Web", "10.0.0.1"));
        store.servers.push(server("evil", "x\n    ProxyCommand sh"));
        let block = render_block(&store);
        assert!(!block.contains("ProxyCommand"));
        assert!(block.contains("Host web\n"));
        assert!(block.contains("Host Web-2\n"));
        // Shared IP stays off the Host lines.
        assert!(!block.contains("Host web 10.0.0.1"));
    }

    #[test]
    fn render_writes_key_and_jump() {
        let mut store = Store::default();
        let key = KeyInfo {
            id: Uuid::new_v4(),
            name: "k".into(),
            algorithm: "ed25519".into(),
            public_openssh: String::new(),
            private_path: "/home/u/.ssh/terminalox_k".into(),
            public_path: String::new(),
            created_at: Utc::now(),
            fingerprint: None,
            origin_imported: false,
            encrypted: false,
        };
        let bastion = server("bastion", "1.2.3.4");
        let mut inner = server("db", "10.0.0.5");
        inner.auth = AuthMode::Key { key_id: key.id };
        inner.jump_server_id = Some(bastion.id);
        inner.port = 2222;
        store.keys.push(key);
        store.servers.push(bastion);
        store.servers.push(inner);
        let block = render_block(&store);
        assert!(block.contains("Host db 10.0.0.5\n    HostName 10.0.0.5\n    User root\n    Port 2222\n    IdentityFile \"/home/u/.ssh/terminalox_k\"\n    IdentitiesOnly yes\n    ProxyJump bastion\n"));
    }

    #[test]
    fn parse_reads_blocks_defaults_and_skips_managed() {
        let text = r#"
Host prod web1
    HostName 192.168.1.10
    User deploy
    Port 2200
    IdentityFile ~/.ssh/id_prod

Host db
  hostname=db.internal
  ProxyJump prod,other

Match host foo
  User nope

Host *
  User fallback
  IdentityFile ~/.ssh/id_default

# BEGIN TERMINALOX
Host managed
# END TERMINALOX
"#;
        let hosts = parse_hosts(text);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].alias, "prod");
        assert_eq!(hosts[0].host, "192.168.1.10");
        assert_eq!(hosts[0].user.as_deref(), Some("deploy"));
        assert_eq!(hosts[0].port, 2200);
        assert!(hosts[0]
            .identity_file
            .as_deref()
            .unwrap()
            .ends_with(".ssh/id_prod"));
        assert_eq!(hosts[1].host, "db.internal");
        assert_eq!(hosts[1].user.as_deref(), Some("fallback"));
        assert_eq!(hosts[1].proxy_jump.as_deref(), Some("prod"));
        assert!(hosts[1]
            .identity_file
            .as_deref()
            .unwrap()
            .ends_with(".ssh/id_default"));
    }
}
