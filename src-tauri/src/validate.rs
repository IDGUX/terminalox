//! Input validation for server entries.
//!
//! Every value here ends up in `~/.ssh/config` and in remote commands, so the
//! rules are strict whitelists: no whitespace, no control characters, no
//! leading `-` (would be read as an option by `ssh`).

use std::net::IpAddr;

use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::Server;
use crate::storage::Store;

const MAX_NAME: usize = 64;
const MAX_HOST: usize = 253;
const MAX_USER: usize = 64;

fn invalid(msg: impl Into<String>) -> AppError {
    AppError::Invalid(msg.into())
}

pub fn name(raw: &str) -> AppResult<String> {
    let v = raw.trim();
    if v.is_empty() {
        return Err(invalid("Name darf nicht leer sein"));
    }
    if v.chars().count() > MAX_NAME {
        return Err(invalid(format!(
            "Name darf höchstens {MAX_NAME} Zeichen lang sein"
        )));
    }
    if v.chars().any(char::is_control) {
        return Err(invalid("Name enthält Steuerzeichen"));
    }
    Ok(v.to_string())
}

pub fn host(raw: &str) -> AppResult<String> {
    let v = raw.trim();
    let v = v
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(v);
    if v.is_empty() {
        return Err(invalid("Host darf nicht leer sein"));
    }
    if v.parse::<IpAddr>().is_ok() {
        return Ok(v.to_string());
    }
    let ok_chars = v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
    if v.len() > MAX_HOST || !ok_chars || v.starts_with(['-', '.']) || v.contains("..") {
        return Err(invalid(format!(
            "„{v}“ ist kein gültiger Hostname und keine IP-Adresse"
        )));
    }
    Ok(v.to_string())
}

pub fn user(raw: &str) -> AppResult<String> {
    let v = raw.trim();
    if v.is_empty() {
        return Err(invalid("Benutzer darf nicht leer sein"));
    }
    let ok_chars = v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@' | '$'));
    if v.len() > MAX_USER || !ok_chars || v.starts_with('-') {
        return Err(invalid(format!("„{v}“ ist kein gültiger Benutzername")));
    }
    Ok(v.to_string())
}

pub fn port(p: u16) -> AppResult<u16> {
    if p == 0 {
        return Err(invalid("Port muss zwischen 1 und 65535 liegen"));
    }
    Ok(p)
}

/// Colors are rendered into inline styles; only `#rgb` / `#rrggbb` pass.
pub fn color(raw: Option<String>) -> AppResult<Option<String>> {
    let Some(c) = raw else { return Ok(None) };
    let hex = c.strip_prefix('#').unwrap_or("");
    if matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(Some(c))
    } else {
        Err(invalid(format!("„{c}“ ist keine gültige Farbe")))
    }
}

/// Server names become `Host` aliases, so they must be unique (case-insensitive).
pub fn unique_name(store: &Store, name: &str, except: Option<Uuid>) -> AppResult<()> {
    let lc = name.to_lowercase();
    if store
        .servers
        .iter()
        .any(|s| Some(s.id) != except && s.name.to_lowercase() == lc)
    {
        return Err(invalid(format!(
            "Ein Server namens „{name}“ existiert bereits"
        )));
    }
    Ok(())
}

/// Validates and normalises all user-controlled fields of a server in place.
pub fn server(s: &mut Server) -> AppResult<()> {
    s.name = name(&s.name)?;
    s.host = host(&s.host)?;
    s.user = user(&s.user)?;
    s.port = port(s.port)?;
    s.color = color(s.color.take())?;
    Ok(())
}

/// A public key line that gets written into `authorized_keys`.
pub fn public_key_line(line: &str) -> AppResult<&str> {
    let v = line.trim();
    if v.is_empty() || v.chars().any(char::is_control) {
        return Err(invalid("ungültige Public-Key-Zeile"));
    }
    russh::keys::PublicKey::from_openssh(v)
        .map_err(|e| invalid(format!("ungültiger Public Key: {e}")))?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_rejects_config_injection() {
        assert!(host("example.com\n    ProxyCommand sh").is_err());
        assert!(host("a b").is_err());
        assert!(host("-oProxyCommand=x").is_err());
        assert!(host("").is_err());
        assert!(host("a..b").is_err());
    }

    #[test]
    fn host_accepts_names_and_ips() {
        assert_eq!(host(" web-1.example.com ").unwrap(), "web-1.example.com");
        assert_eq!(host("10.0.0.1").unwrap(), "10.0.0.1");
        assert_eq!(host("[2001:db8::1]").unwrap(), "2001:db8::1");
        assert_eq!(host("fe80::1").unwrap(), "fe80::1");
    }

    #[test]
    fn user_rules() {
        assert!(user("root").is_ok());
        assert!(user("deploy.bot@corp").is_ok());
        assert!(user("ro ot").is_err());
        assert!(user("-l").is_err());
        assert!(user("x\ny").is_err());
    }

    #[test]
    fn color_rules() {
        assert_eq!(color(None).unwrap(), None);
        assert!(color(Some("#a1b2c3".into())).is_ok());
        assert!(color(Some("#abc".into())).is_ok());
        assert!(color(Some("red;background:url(x)".into())).is_err());
    }

    #[test]
    fn name_rules() {
        assert_eq!(name("  Prod DB ").unwrap(), "Prod DB");
        assert!(name("   ").is_err());
        assert!(name("a\nb").is_err());
    }
}
