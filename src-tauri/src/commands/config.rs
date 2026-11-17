use std::path::Path;

use chrono::Utc;
use russh::keys::ssh_key::PrivateKey;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::error::AppResult;
use crate::keygen;
use crate::models::{AuthMode, Server};
use crate::msg::Msg;
use crate::ssh_config::{self, ParsedHost};
use crate::state::AppState;
use crate::storage::Store;
use crate::validate;

#[tauri::command]
pub async fn sync_ssh_config(state: State<'_, AppState>) -> AppResult<String> {
    let store = state.store.read().await;
    let path = ssh_config::sync(&store)?;
    Ok(path.to_string_lossy().to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    #[serde(flatten)]
    pub host: ParsedHost,
    /// User that will be used (config value or the local login name).
    pub effective_user: String,
    /// A server with this name already exists in terminalox.
    pub already_exists: bool,
    /// Why it cannot be imported, if so.
    pub problem: Option<Msg>,
    /// How login will work: "key", "agent" or "password".
    pub auth: &'static str,
}

fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".into())
}

/// Identity file for a host: the configured one, else the first OpenSSH default.
fn identity_for(h: &ParsedHost) -> Option<String> {
    if let Some(f) = &h.identity_file {
        return Path::new(f).exists().then(|| f.clone());
    }
    let dir = keygen::ssh_dir().ok()?;
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
}

fn read_config_hosts() -> AppResult<Vec<ParsedHost>> {
    let path = ssh_config::config_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    Ok(ssh_config::parse_hosts(&std::fs::read_to_string(path)?))
}

fn candidate(store: &Store, h: ParsedHost) -> ImportCandidate {
    let effective_user = h.user.clone().unwrap_or_else(local_user);
    let problem = validate::host(&h.host)
        .and_then(|_| validate::user(&effective_user))
        .and_then(|_| validate::name(&h.alias))
        .err()
        .map(|e| e.to_msg());
    let already_exists = validate::unique_name(store, &h.alias, None).is_err();
    let auth = if identity_for(&h).is_some() {
        "key"
    } else {
        "password"
    };
    ImportCandidate {
        host: h,
        effective_user,
        already_exists,
        problem,
        auth,
    }
}

/// Lists the hosts of `~/.ssh/config` that could be imported.
#[tauri::command]
pub async fn preview_ssh_config_import(
    state: State<'_, AppState>,
) -> AppResult<Vec<ImportCandidate>> {
    let hosts = read_config_hosts()?;
    let store = state.store.read().await;
    Ok(hosts.into_iter().map(|h| candidate(&store, h)).collect())
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigImportReport {
    pub servers_added: usize,
    pub keys_imported: usize,
    pub warnings: Vec<HostWarning>,
}

/// A problem with one host (`alias`) during import.
#[derive(Debug, Serialize)]
pub struct HostWarning {
    pub alias: String,
    pub msg: Msg,
}

impl ConfigImportReport {
    fn warn(&mut self, alias: &str, msg: Msg) {
        self.warnings.push(HostWarning {
            alias: alias.to_string(),
            msg,
        });
    }
}

/// Finds an already imported key with the same fingerprint, or imports the file.
fn key_for_file(
    store: &mut Store,
    alias: &str,
    path: &str,
    report: &mut ConfigImportReport,
) -> Option<Uuid> {
    let content = std::fs::read_to_string(path).ok()?;
    let key = PrivateKey::from_openssh(&content).ok()?;
    let fp = keygen::fingerprint_of(key.public_key());
    if let Some(k) = store
        .keys
        .iter()
        .find(|k| k.fingerprint.as_deref() == Some(fp.as_str()))
    {
        return Some(k.id);
    }
    let stem = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "key".into());
    let names: Vec<String> = std::iter::once(stem.clone())
        .chain((2..100).map(|i| format!("{stem}_{i}")))
        .collect();
    for name in names {
        match keygen::import_from_file(Path::new(path), &name) {
            Ok(info) => {
                let id = info.id;
                store.keys.push(info);
                report.keys_imported += 1;
                return Some(id);
            }
            Err(crate::error::AppError::Msg(m)) if m.code == "key_exists" => continue,
            Err(e) => {
                report.warn(
                    alias,
                    msg!(
                        "import_key_failed",
                        "Key {path} not imported: {detail}",
                        path = path,
                        detail = e
                    ),
                );
                return None;
            }
        }
    }
    None
}

/// Resolves a `ProxyJump` value (alias or `user@host:port`) to a server entry.
fn resolve_jump(store: &Store, spec: &str) -> Option<Uuid> {
    if let Some(s) = store
        .servers
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(spec))
    {
        return Some(s.id);
    }
    let (user, rest) = match spec.split_once('@') {
        Some((u, r)) => (Some(u), r),
        None => (None, spec),
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => (h, p.parse().ok()?),
        _ => (rest, 22),
    };
    store
        .servers
        .iter()
        .find(|s| {
            s.host.eq_ignore_ascii_case(host) && s.port == port && user.is_none_or(|u| s.user == u)
        })
        .map(|s| s.id)
}

#[tauri::command]
pub async fn import_ssh_config_hosts(
    aliases: Vec<String>,
    state: State<'_, AppState>,
) -> AppResult<ConfigImportReport> {
    let hosts = read_config_hosts()?;
    let mut store = state.store.write().await;
    let mut report = ConfigImportReport::default();
    let mut added: Vec<(Uuid, Option<String>)> = vec![];

    for h in hosts.into_iter().filter(|h| aliases.contains(&h.alias)) {
        let c = candidate(&store, h.clone());
        if let Some(p) = c.problem {
            report.warn(&h.alias, p);
            continue;
        }
        if c.already_exists {
            report.warn(&h.alias, msg!("import_exists", "Already exists, skipped"));
            continue;
        }
        let auth = identity_for(&h)
            .and_then(|f| key_for_file(&mut store, &h.alias, &f, &mut report))
            .map(|key_id| AuthMode::Key { key_id })
            .unwrap_or(AuthMode::Password);
        let mut server = Server {
            id: Uuid::new_v4(),
            name: h.alias.clone(),
            host: h.host.clone(),
            port: h.port,
            user: c.effective_user,
            auth,
            color: None,
            created_at: Utc::now(),
            last_used_at: None,
            hardened: false,
            hardened_at: None,
            folder_id: None,
            jump_server_id: None,
        };
        if let Err(e) = validate::server(&mut server) {
            report.warn(&h.alias, e.to_msg());
            continue;
        }
        added.push((server.id, h.proxy_jump.clone()));
        store.servers.push(server);
        report.servers_added += 1;
    }

    // Jumps last, so hosts can reference each other regardless of order.
    for (id, jump) in added {
        let Some(spec) = jump else { continue };
        match resolve_jump(&store, &spec) {
            Some(jid) if jid != id => {
                if let Some(s) = store.find_server_mut(id) {
                    s.jump_server_id = Some(jid);
                }
            }
            _ => {
                let name = store
                    .find_server(id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                report.warn(
                    &name,
                    msg!(
                        "import_jump_missing",
                        "Jump host \"{spec}\" is not set up in terminalox, please add it",
                        spec = spec
                    ),
                );
            }
        }
    }

    store.save(&state.store_file())?;
    Ok(report)
}
