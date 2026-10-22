//! File search (plan §6.6) through the Windows Search index: the query sent
//! to it, and the order results are shown in.
//!
//! Windows-free and host-testable. smolcast keeps no index of its own: the
//! Windows Search service already indexes the user's folders, and
//! `win/file_search.rs` sends it the SQL built here. Each typed word must
//! start a word of the file name (`rep 24` finds `Annual report 2024.pdf`),
//! which is how Explorer's search box matches names too.

/// Results asked for per query.
pub const MAX_RESULTS: usize = 40;
/// Longest query sent to the index, in characters.
pub const MAX_QUERY_CHARS: usize = 200;

/// One file or folder the index returned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileHit {
    /// Full path, as Explorer shows it.
    pub path: String,
    /// The file name with its extension.
    pub name: String,
    pub folder: bool,
}

impl FileHit {
    pub fn from_path(path: String, folder: bool) -> Self {
        let name = path
            .rsplit(['\\', '/'])
            .find(|part| !part.is_empty())
            .unwrap_or(&path)
            .to_owned();
        Self { path, name, folder }
    }

    /// The folder holding the item, for the row's second column.
    pub fn parent(&self) -> &str {
        let trimmed = self.path.trim_end_matches(['\\', '/']);
        trimmed
            .rfind(['\\', '/'])
            .map_or("", |i| &trimmed[..i.max(1)])
    }
}

/// The Windows Search SQL for `query`, or `None` when it has no searchable
/// word. Words are matched as name prefixes, all of them required.
pub fn sql(query: &str) -> Option<String> {
    let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|word| {
            // `"` and `*` are CONTAINS syntax; `'` ends the SQL literal.
            word.chars()
                .filter(|c| !matches!(c, '"' | '*') && !c.is_control())
                .collect::<String>()
                .replace('\'', "''")
        })
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .map(|word| format!("\"{word}*\""))
        .collect();
    if terms.is_empty() {
        return None;
    }
    Some(format!(
        "SELECT TOP {MAX_RESULTS} System.ItemPathDisplay, System.ItemType FROM SystemIndex \
         WHERE SCOPE='file:' AND CONTAINS(System.FileName, '{}') \
         ORDER BY System.Search.Rank DESC",
        terms.join(" AND ")
    ))
}

/// Keep the index's ranking, but move names that start with the whole query
/// to the front, as a launcher user expects.
pub fn rerank(hits: &mut [FileHit], query: &str) {
    let query = query.trim().to_lowercase();
    hits.sort_by_cached_key(|hit| !hit.name.to_lowercase().starts_with(&query));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_prefix_query_over_file_names() {
        assert_eq!(
            sql("  annual   rep ").unwrap(),
            "SELECT TOP 40 System.ItemPathDisplay, System.ItemType FROM SystemIndex \
             WHERE SCOPE='file:' AND CONTAINS(System.FileName, '\"annual*\" AND \"rep*\"') \
             ORDER BY System.Search.Rank DESC"
        );
    }

    #[test]
    fn query_text_cannot_escape_the_literal() {
        let built = sql("it's \"x\" a*b").unwrap();
        assert!(
            built.contains("'\"it''s*\" AND \"x*\" AND \"ab*\"'"),
            "{built}"
        );
        assert_eq!(sql(""), None);
        assert_eq!(sql(" - ** \"\" "), None);
        // Over-long queries are cut, never rejected.
        assert!(sql(&"a".repeat(1000)).unwrap().len() < 400);
    }

    #[test]
    fn hits_know_their_name_and_folder() {
        let hit = FileHit::from_path("C:\\Users\\me\\Report.pdf".into(), false);
        assert_eq!(hit.name, "Report.pdf");
        assert_eq!(hit.parent(), "C:\\Users\\me");
        let folder = FileHit::from_path("C:\\Projects\\".into(), true);
        assert_eq!(folder.name, "Projects");
        assert_eq!(folder.parent(), "C:");
    }

    #[test]
    fn names_starting_with_the_query_come_first() {
        let mut hits = vec![
            FileHit::from_path("C:\\a\\old report.txt".into(), false),
            FileHit::from_path("C:\\b\\Report.pdf".into(), false),
            FileHit::from_path("C:\\c\\report notes".into(), true),
        ];
        rerank(&mut hits, "report");
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["Report.pdf", "report notes", "old report.txt"]);
    }
}
