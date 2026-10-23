//! Quicklinks (plan §10.7): a named URL, file or folder opened from the
//! palette, optionally with a query typed after the quicklink's id.
//!
//! Windows-free and host-testable. A target containing `{query}` takes a
//! query: `gh rust egui` opens the `gh` quicklink with "rust egui" in place
//! of `{query}`. The query is percent-encoded unless the target is a
//! filesystem path, where it is inserted as typed.

use crate::core::web;

/// Placeholder in a quicklink's `url` replaced by the typed query.
pub const QUERY_PLACEHOLDER: &str = "{query}";

/// Whether a quicklink target needs a query before it can open.
pub fn takes_query(target: &str) -> bool {
    target.contains(QUERY_PLACEHOLDER)
}

/// `target` with every `{query}` replaced by `query`.
pub fn expand(target: &str, query: &str) -> String {
    let query = query.trim();
    let value = if is_path(target) {
        query.to_owned()
    } else {
        web::encode(query)
    };
    target.replace(QUERY_PLACEHOLDER, &value)
}

/// A drive (`C:\`), UNC (`\\server`) or `%VAR%\` path rather than a URL.
fn is_path(target: &str) -> bool {
    let bytes = target.as_bytes();
    target.starts_with("\\\\")
        || leading_variable(target).is_some()
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

/// The name in a leading `%NAME%` followed by a path separator
/// (`%USERPROFILE%\Downloads`). A URL's `%20` never matches: no closing `%`
/// comes right before a separator.
fn leading_variable(target: &str) -> Option<&str> {
    let rest = target.strip_prefix('%')?;
    let end = rest.find('%')?;
    let name = &rest[..end];
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'(' | b')'));
    (valid && matches!(rest.as_bytes().get(end + 1), Some(b'\\' | b'/'))).then_some(name)
}

/// `target` with a leading `%NAME%` replaced by `lookup(NAME)`, as Explorer
/// reads such a path. Only that variable is expanded; one `lookup` does not
/// know is left as typed.
pub fn expand_variable(target: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    match leading_variable(target).and_then(|name| lookup(name).map(|value| (name, value))) {
        Some((name, value)) => format!("{value}{}", &target[name.len() + 2..]),
        None => target.to_owned(),
    }
}

/// Split a root query into a quicklink keyword and the query after it:
/// `"gh rust egui"` is `("gh", "rust egui")`. `None` without a space or with
/// nothing after it.
pub fn split_keyword(query: &str) -> Option<(&str, &str)> {
    let query = query.trim_start();
    let (keyword, rest) = query.split_once(char::is_whitespace)?;
    let rest = rest.trim();
    (!keyword.is_empty() && !rest.is_empty()).then_some((keyword, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_get_an_encoded_query() {
        let gh = "https://github.com/search?q={query}&type=code";
        assert!(takes_query(gh));
        assert_eq!(
            expand(gh, " rust & egui "),
            "https://github.com/search?q=rust%20%26%20egui&type=code"
        );
        assert_eq!(expand("ms-settings:display", "x"), "ms-settings:display");
        assert!(!takes_query("ms-settings:display"));
    }

    #[test]
    fn paths_get_the_query_as_typed() {
        assert_eq!(
            expand("C:\\Projects\\{query}", "smolcast docs"),
            "C:\\Projects\\smolcast docs"
        );
        assert_eq!(
            expand("\\\\nas\\share\\{query}", "a b"),
            "\\\\nas\\share\\a b"
        );
        assert_eq!(expand("C:/x/{query}", "a b"), "C:/x/a b");
        // A scheme that happens to be one letter long is not a drive.
        assert_eq!(expand("c:{query}", "a b"), "c:a%20b");
    }

    #[test]
    fn keyword_needs_a_query_after_it() {
        assert_eq!(split_keyword("gh rust egui"), Some(("gh", "rust egui")));
        assert_eq!(split_keyword("  gh   x  "), Some(("gh", "x")));
        assert_eq!(split_keyword("gh"), None);
        assert_eq!(split_keyword("gh   "), None);
        assert_eq!(split_keyword(""), None);
    }
}
