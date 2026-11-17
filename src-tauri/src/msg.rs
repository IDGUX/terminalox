//! User-facing messages with a stable code.
//!
//! The frontend translates `code` + `params` into the UI language; `text` is
//! the English fallback for codes it does not know (and for logs).

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Msg {
    pub code: &'static str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<&'static str, String>,
    pub text: String,
}

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// `msg!("code", "English text with {name}", name = value)`.
/// Every parameter must appear in the text.
#[macro_export]
macro_rules! msg {
    ($code:literal, $text:literal $(, $k:ident = $v:expr)* $(,)?) => {{
        #[allow(unused_mut)]
        let mut params = std::collections::BTreeMap::new();
        $(
            let $k = ($v).to_string();
            params.insert(stringify!($k), $k.clone());
        )*
        $crate::msg::Msg {
            code: $code,
            text: format!($text $(, $k = $k)*),
            params,
        }
    }};
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_params_and_text() {
        let m = msg!("invalid_host", "\"{value}\" is not valid", value = "a b");
        assert_eq!(m.code, "invalid_host");
        assert_eq!(m.text, "\"a b\" is not valid");
        assert_eq!(m.params["value"], "a b");
    }
}
