//! Web search fallback: the search URL for a query.
//!
//! Windows-free and host-testable. Windows has no system-wide default
//! search engine (each browser keeps its own), so the engine is a setting:
//! one of the major engines, or a custom URL template. The default browser
//! opens the result.

use std::fmt::Write;

use serde::{Deserialize, Serialize};

/// The `search_engine` setting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    #[default]
    Google,
    Bing,
    DuckDuckGo,
    Brave,
    Ecosia,
    Startpage,
    Kagi,
    /// `web_search_url` from the settings.
    Custom,
}

impl SearchEngine {
    pub const ALL: [Self; 8] = [
        Self::Google,
        Self::Bing,
        Self::DuckDuckGo,
        Self::Brave,
        Self::Ecosia,
        Self::Startpage,
        Self::Kagi,
        Self::Custom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Bing => "Bing",
            Self::DuckDuckGo => "DuckDuckGo",
            Self::Brave => "Brave Search",
            Self::Ecosia => "Ecosia",
            Self::Startpage => "Startpage",
            Self::Kagi => "Kagi",
            Self::Custom => "Custom URL",
        }
    }

    /// The engine's URL template; `None` for `Custom`.
    pub fn template(self) -> Option<&'static str> {
        Some(match self {
            Self::Google => "https://www.google.com/search?q=%s",
            Self::Bing => "https://www.bing.com/search?q=%s",
            Self::DuckDuckGo => "https://duckduckgo.com/?q=%s",
            Self::Brave => "https://search.brave.com/search?q=%s",
            Self::Ecosia => "https://www.ecosia.org/search?q=%s",
            Self::Startpage => "https://www.startpage.com/do/search?q=%s",
            Self::Kagi => "https://kagi.com/search?q=%s",
            Self::Custom => return None,
        })
    }

    /// The URL template in effect, given the custom one from the settings.
    pub fn template_or(self, custom: &str) -> &str {
        self.template().unwrap_or(custom)
    }
}

/// Placeholder in `web_search_url` replaced by the encoded query.
pub const QUERY_PLACEHOLDER: &str = "%s";
pub const DEFAULT_SEARCH_URL: &str = "https://www.google.com/search?q=%s";
pub const MAX_SEARCH_URL_CHARS: usize = 2048;

/// Why a search URL template is unusable, as a plain message.
pub fn template_problem(template: &str) -> Option<&'static str> {
    if !(template.starts_with("https://") || template.starts_with("http://")) {
        Some("must start with https:// or http://")
    } else if !template.contains(QUERY_PLACEHOLDER) {
        Some("must contain %s where the query goes")
    } else if template.chars().count() > MAX_SEARCH_URL_CHARS {
        Some("is too long")
    } else if template.chars().any(char::is_whitespace) {
        Some("must not contain spaces")
    } else {
        None
    }
}

/// `template` with every `%s` replaced by the percent-encoded query.
pub fn search_url(template: &str, query: &str) -> String {
    template.replace(QUERY_PLACEHOLDER, &encode(query.trim()))
}

/// Percent-encode UTF-8, keeping only RFC 3986 unreserved characters.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            // Writing into a String cannot fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_query_into_the_template() {
        assert_eq!(
            search_url(DEFAULT_SEARCH_URL, "  rust & egui?  "),
            "https://www.google.com/search?q=rust%20%26%20egui%3F"
        );
        assert_eq!(
            search_url("https://duckduckgo.com/?q=%s", "café ☕"),
            "https://duckduckgo.com/?q=caf%C3%A9%20%E2%98%95"
        );
        assert_eq!(
            search_url("https://x.test/%s/%s", "a b"),
            "https://x.test/a%20b/a%20b"
        );
    }

    #[test]
    fn every_preset_has_a_valid_template() {
        for engine in SearchEngine::ALL {
            match engine.template() {
                Some(template) => assert_eq!(template_problem(template), None, "{engine:?}"),
                None => assert_eq!(engine, SearchEngine::Custom),
            }
        }
        assert_eq!(
            SearchEngine::Custom.template_or("https://x.test/?q=%s"),
            "https://x.test/?q=%s"
        );
        assert_eq!(
            SearchEngine::Bing.template_or("https://x.test/?q=%s"),
            "https://www.bing.com/search?q=%s"
        );
    }

    #[test]
    fn validates_templates() {
        assert_eq!(template_problem(DEFAULT_SEARCH_URL), None);
        assert!(template_problem("https://bing.com/search?q=").is_some());
        assert!(template_problem("file:///C:/x?q=%s").is_some());
        assert!(template_problem("javascript:alert(%s)").is_some());
        assert!(template_problem("https://x.test/?q=%s &a").is_some());
    }
}
