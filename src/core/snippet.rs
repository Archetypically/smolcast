//! Snippets (plan §10.6): the validated table from config, deterministic search,
//! and the exact text a paste writes.
//!
//! Windows-free and host-testable. The palette view is the only consumer today;
//! a future as-you-type expander would read the same table and use `keyword`
//! as its trigger.

use std::collections::BTreeMap;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::core::config::{self, SnippetConfig};

/// One configured snippet, owned by the view.
#[derive(Clone, Debug)]
pub struct Snippet {
    /// The `[snippets.<id>]` table key.
    pub id: String,
    pub name: String,
    pub keyword: Option<String>,
    pub text: String,
    /// `text`, `name` and `keyword` lowercased once for search.
    text_lower: String,
    name_lower: String,
    keyword_lower: Option<String>,
    /// Unicode scalar values in `text`, for the preview.
    pub chars: usize,
}

impl Snippet {
    fn new(id: String, name: String, keyword: Option<String>, text: String) -> Self {
        Self {
            text_lower: text.to_lowercase(),
            name_lower: name.to_lowercase(),
            keyword_lower: keyword.as_deref().map(str::to_lowercase),
            chars: text.chars().count(),
            id,
            name,
            keyword,
            text,
        }
    }
}

/// Snippets in config order (sorted by id).
pub fn from_config(snippets: &BTreeMap<String, SnippetConfig>) -> Vec<Snippet> {
    snippets
        .iter()
        .map(|(id, s)| {
            Snippet::new(
                id.clone(),
                s.name.clone(),
                s.keyword.clone(),
                s.text.clone(),
            )
        })
        .collect()
}

/// Table id for a new snippet named `name` ([`config::new_id`]), unique
/// among `snippets`.
pub fn new_id(name: &str, snippets: &[Snippet]) -> String {
    config::new_id(name, "snippet", snippets.iter().map(|s| s.id.as_str()))
}

/// Why `draft` cannot be saved: a field rule, or a keyword another snippet
/// already uses (ignoring case). `editing` is the id being replaced, if any.
pub fn draft_problem(
    draft: &SnippetConfig,
    editing: Option<&str>,
    snippets: &[Snippet],
) -> Option<String> {
    if let Some(problem) = draft.problem() {
        return Some(problem);
    }
    let keyword = draft.keyword.as_deref()?.to_lowercase();
    snippets
        .iter()
        .filter(|s| Some(s.id.as_str()) != editing)
        .find(|s| s.keyword_lower.as_deref() == Some(keyword.as_str()))
        .map(|s| format!("keyword is already used by \"{}\"", s.name))
}

/// How a snippet matched; declaration order is rank order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Tier {
    KeywordExact,
    KeywordPrefix,
    Fuzzy,
    Body,
}

/// Indices into `snippets` for `query` (trimmed), best first.
///
/// Order: an exact keyword match (ignoring case), then keyword prefixes, then
/// fuzzy matches on the name or keyword by score, then snippets whose text
/// contains the query. Ties sort by lowercase name, then id. An empty query
/// lists everything by name.
pub fn search(query: &str, snippets: &[Snippet]) -> Vec<usize> {
    let query = query.trim();
    let needle = query.to_lowercase();
    let matcher = SkimMatcherV2::default();

    let mut hits: Vec<(Tier, i64, &str, usize)> = snippets
        .iter()
        .enumerate()
        .filter_map(|(index, s)| {
            let (tier, score) = if query.is_empty() {
                (Tier::Fuzzy, 0)
            } else {
                classify(s, query, &needle, &matcher)?
            };
            Some((tier, score, s.name_lower.as_str(), index))
        })
        .collect();
    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(b.2))
            .then_with(|| snippets[a.3].id.cmp(&snippets[b.3].id))
    });
    hits.into_iter().map(|(.., index)| index).collect()
}

fn classify(
    s: &Snippet,
    query: &str,
    needle: &str,
    matcher: &SkimMatcherV2,
) -> Option<(Tier, i64)> {
    if let Some(keyword) = s.keyword_lower.as_deref() {
        if keyword == needle {
            return Some((Tier::KeywordExact, 0));
        }
        if keyword.starts_with(needle) {
            return Some((Tier::KeywordPrefix, 0));
        }
    }
    let fuzzy = std::iter::once(s.name.as_str())
        .chain(s.keyword.as_deref())
        .filter_map(|field| matcher.fuzzy_match(field, query))
        .max();
    if let Some(score) = fuzzy {
        return Some((Tier::Fuzzy, score));
    }
    s.text_lower.contains(needle).then_some((Tier::Body, 0))
}

/// The text a paste writes: every line break becomes CRLF, the Windows text
/// convention (TOML files may use either).
pub fn paste_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push_str("\r\n");
            }
            '\n' => out.push_str("\r\n"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(id: &str, name: &str, keyword: Option<&str>, text: &str) -> Snippet {
        Snippet::new(id.into(), name.into(), keyword.map(Into::into), text.into())
    }

    fn ids(query: &str, snippets: &[Snippet]) -> Vec<String> {
        search(query, snippets)
            .into_iter()
            .map(|i| snippets[i].id.clone())
            .collect()
    }

    #[test]
    fn empty_query_lists_all_by_name_then_id() {
        let snippets = [
            snippet("b", "beta", None, "x"),
            snippet("a2", "Alpha", None, "x"),
            snippet("a1", "alpha", None, "x"),
        ];
        assert_eq!(ids("  ", &snippets), ["a1", "a2", "b"]);
    }

    #[test]
    fn keyword_exact_beats_prefix_beats_fuzzy_beats_body() {
        let snippets = [
            snippet("body", "Greeting", None, "see ;sig below"),
            snippet("fuzzy", "Short signature", None, "x"),
            snippet("prefix", "Long signature", Some(";sig-long"), "x"),
            snippet("exact", "Zed", Some(";SIG"), "x"),
        ];
        assert_eq!(ids(";sig", &snippets), ["exact", "prefix", "body"]);
        // Without the `;`, only fuzzy and body tiers apply; body comes last.
        let loose = ids("sig", &snippets);
        assert_eq!(loose.len(), 4);
        assert_eq!(loose.last().map(String::as_str), Some("body"));
    }

    #[test]
    fn body_match_ignores_case_and_misses_are_dropped() {
        let snippets = [
            snippet("addr", "Address", None, "221B Baker Street"),
            snippet("other", "Other", None, "nothing here"),
        ];
        assert_eq!(ids("baker street", &snippets), ["addr"]);
        assert!(ids("qqq", &snippets).is_empty());
    }

    #[test]
    fn new_ids_are_valid_slugs_and_unique_ignoring_case() {
        let snippets = [
            snippet("email-signature", "x", None, "x"),
            snippet("Email-Signature-2", "x", None, "x"),
        ];
        assert_eq!(new_id("Email signature!", &snippets), "email-signature-3");
        assert_eq!(new_id("  Déjà vu ✓ ", &snippets), "d-j-vu");
        assert_eq!(new_id("✓✓✓", &snippets), "snippet");
        let long = new_id(&"ab ".repeat(40), &snippets);
        assert!(long.len() <= 48 && !long.ends_with('-'), "{long}");
        for id in ["email-signature-3", "d-j-vu", "snippet", long.as_str()] {
            assert!(crate::core::config::is_valid_id(id), "{id}");
        }
    }

    #[test]
    fn draft_problem_reports_fields_and_keyword_collisions() {
        let snippets = [snippet("sig", "Signature", Some(";Sig"), "x")];
        let draft = |keyword: Option<&str>| SnippetConfig {
            name: "New".into(),
            text: "t".into(),
            keyword: keyword.map(Into::into),
        };
        assert_eq!(
            draft_problem(&draft(Some(";sig")), None, &snippets).as_deref(),
            Some("keyword is already used by \"Signature\"")
        );
        // Editing the snippet that owns the keyword is not a collision.
        assert_eq!(
            draft_problem(&draft(Some(";sig")), Some("sig"), &snippets),
            None
        );
        assert_eq!(draft_problem(&draft(None), None, &snippets), None);
        let empty_name = SnippetConfig {
            name: String::new(),
            ..draft(None)
        };
        assert_eq!(
            draft_problem(&empty_name, None, &snippets).as_deref(),
            Some("name must be nonempty")
        );
    }

    #[test]
    fn paste_text_normalizes_every_line_break_to_crlf() {
        assert_eq!(paste_text("a\nb"), "a\r\nb");
        assert_eq!(paste_text("a\r\nb"), "a\r\nb");
        assert_eq!(paste_text("a\rb\r"), "a\r\nb\r\n");
        assert_eq!(paste_text("a\n\r\nb ✓"), "a\r\n\r\nb ✓");
        assert_eq!(paste_text("plain"), "plain");
    }
}
