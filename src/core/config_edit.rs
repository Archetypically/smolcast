//! Form writes to `config.toml`: the settings form (plan §12.7), the snippet
//! editor (§10.6), and the script editor (§10.3). The file stays the single
//! source of truth: each edit touches only the keys whose values changed,
//! preserving comments, formatting, key order, and every other table, and the
//! result is re-validated and round-trip checked before the caller writes it.
//!
//! The §10.2 rules hold: an existing file that does not parse or validate
//! (including a newer `schema_version`) is never overwritten, and an absent
//! file is created only by an explicit save.

use std::collections::BTreeMap;
use std::path::Path;

use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike, Value};

use super::config::{
    ConfigError, ConfigFile, SCHEMA_VERSION, ScriptConfig, Settings, SnippetConfig, parse_str,
    validation,
};

/// One form save.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigEdit {
    Settings(Settings),
    Snippet(SnippetEdit),
    Script(ScriptEdit),
}

/// A script-editor save (§10.3), shaped like [`SnippetEdit`].
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptEdit {
    Put {
        id: String,
        script: ScriptConfig,
        create: bool,
    },
    Delete {
        id: String,
    },
}

/// A snippet-editor save (§10.6).
#[derive(Clone, Debug, PartialEq)]
pub enum SnippetEdit {
    /// Write `[snippets.<id>]`. `create` requires the id to be new; otherwise
    /// the id must exist, and only its changed fields are rewritten.
    Put {
        id: String,
        snippet: SnippetConfig,
        create: bool,
    },
    Delete {
        id: String,
    },
}

/// Return the new file text with `edit` applied. `existing` is the current
/// file text, or `None` when the file is absent.
pub fn apply(
    existing: Option<&str>,
    edit: &ConfigEdit,
    path: &Path,
) -> Result<String, ConfigError> {
    match edit {
        ConfigEdit::Settings(settings) => write_settings(existing, settings, path),
        ConfigEdit::Snippet(edit) => write_snippet(existing, edit, path),
        ConfigEdit::Script(edit) => write_script(existing, edit, path),
    }
}

/// The document for an absent file: the schema version and every settings key.
fn new_document(settings: &Settings, path: &Path) -> Result<DocumentMut, ConfigError> {
    // Serializing the struct keeps its declaration order.
    let body = toml::to_string(settings).map_err(|e| validation(path, e.to_string()))?;
    format!("schema_version = {SCHEMA_VERSION}\n\n[settings]\n{body}")
        .parse::<DocumentMut>()
        .map_err(|e| validation(path, e.to_string()))
}

/// Parse existing text for editing. Refuses malformed and newer-schema files
/// before touching them.
fn open_document(text: &str, path: &Path) -> Result<(DocumentMut, ConfigFile), ConfigError> {
    let current = parse_str(text, path)?;
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| validation(path, e.to_string()))?;
    Ok((doc, current))
}

/// Return the new file text with `settings` applied. `existing` is the current
/// file text, or `None` when the file is absent.
pub fn write_settings(
    existing: Option<&str>,
    settings: &Settings,
    path: &Path,
) -> Result<String, ConfigError> {
    let invalid = |message: String| validation(path, message);
    settings.validate(path)?;

    let (mut doc, current) = match existing {
        Some(text) => {
            let (doc, current) = open_document(text, path)?;
            (doc, Some(current.settings))
        }
        None => (new_document(settings, path)?, None),
    };

    let new = to_table(settings, path)?;
    let old = current
        .as_ref()
        .map(|current| to_table(current, path))
        .transpose()?;
    // `settings = { … }` is as valid as a `[settings]` header.
    let table = doc["settings"]
        .as_table_like_mut()
        .ok_or_else(|| invalid("settings is not a table".into()))?;
    for (key, value) in &new {
        if old.as_ref().and_then(|old| old.get(key)) == Some(value) {
            continue;
        }
        let replacement = value
            .to_string()
            .parse::<Value>()
            .map_err(|e| invalid(e.to_string()))?;
        set_value(table, key, replacement);
    }
    // An optional key cleared in the form (`None`) is left out of `new`.
    if let Some(old) = &old {
        for key in old.keys().filter(|key| !new.contains_key(key.as_str())) {
            table.remove(key);
        }
    }

    let text = doc.to_string();
    let written: ConfigFile = parse_str(&text, path)?;
    if &written.settings != settings {
        return Err(invalid("edited file did not round-trip".into()));
    }
    Ok(text)
}

/// Return the new file text with one snippet created, changed, or removed.
/// Other snippets, scripts, and settings are left byte-identical.
pub fn write_snippet(
    existing: Option<&str>,
    edit: &SnippetEdit,
    path: &Path,
) -> Result<String, ConfigError> {
    let entry = match edit {
        SnippetEdit::Put {
            id,
            snippet,
            create,
        } => (id, Some((snippet, *create))),
        SnippetEdit::Delete { id } => (id, None),
    };
    write_entry(
        existing,
        path,
        Kind {
            name: "snippet",
            key: "snippets",
            map: |file| &mut file.snippets,
            set_fields: set_snippet_fields,
        },
        entry,
    )
}

/// Return the new file text with one script created, changed, or removed
/// (§10.3). Other scripts, snippets, and settings are left byte-identical.
pub fn write_script(
    existing: Option<&str>,
    edit: &ScriptEdit,
    path: &Path,
) -> Result<String, ConfigError> {
    let entry = match edit {
        ScriptEdit::Put { id, script, create } => (id, Some((script, *create))),
        ScriptEdit::Delete { id } => (id, None),
    };
    write_entry(
        existing,
        path,
        Kind {
            name: "script",
            key: "scripts",
            map: |file| &mut file.scripts,
            set_fields: set_script_fields,
        },
        entry,
    )
}

/// A `[<key>.<id>]` table family the forms edit.
struct Kind<T> {
    /// Singular, for messages.
    name: &'static str,
    /// The top-level table.
    key: &'static str,
    map: fn(&mut ConfigFile) -> &mut BTreeMap<String, T>,
    set_fields: fn(&mut dyn TableLike, &T, Option<&T>),
}

/// Create or change (`Some((value, create))`) or remove (`None`) one entry.
/// `create` requires the id to be new; otherwise the id must exist, and only
/// its changed fields are rewritten.
fn write_entry<T: Clone>(
    existing: Option<&str>,
    path: &Path,
    kind: Kind<T>,
    (id, put): (&String, Option<(&T, bool)>),
) -> Result<String, ConfigError> {
    let (mut doc, mut expected) = match existing {
        Some(text) => open_document(text, path)?,
        None => (
            new_document(&Settings::default(), path)?,
            ConfigFile::default(),
        ),
    };
    let name = kind.name;
    let gone = || validation(path, format!("{name} {id:?} is no longer in the file"));

    match put {
        Some((value, create)) => {
            let old = (kind.map)(&mut expected).insert(id.clone(), value.clone());
            match (&old, create) {
                (Some(_), true) => {
                    return Err(validation(path, format!("{name} {id:?} already exists")));
                }
                (None, false) => return Err(gone()),
                _ => {}
            }
            // Field rules and whole-file rules, before touching the document.
            expected.validate(path)?;
            let (entries, inline) = entries_table(&mut doc, kind.key, path)?;
            if let Some(item) = entries.get_mut(id) {
                let table = item
                    .as_table_like_mut()
                    .ok_or_else(|| validation(path, format!("{name} {id:?} is not a table")))?;
                (kind.set_fields)(table, value, old.as_ref());
            } else if inline {
                let mut table = InlineTable::new();
                (kind.set_fields)(&mut table, value, None);
                entries.insert(id, Item::Value(Value::InlineTable(table)));
            } else {
                let mut table = Table::new();
                (kind.set_fields)(&mut table, value, None);
                entries.insert(id, Item::Table(table));
            }
        }
        None => {
            if (kind.map)(&mut expected).remove(id).is_none() {
                return Err(gone());
            }
            entries_table(&mut doc, kind.key, path)?.0.remove(id);
        }
    }

    let text = doc.to_string();
    if parse_str(&text, path)? != expected {
        return Err(validation(path, "edited file did not round-trip".into()));
    }
    Ok(text)
}

/// The `[<key>]` table, created implicitly (no bare header) when absent,
/// and whether it is written inline (`snippets = { … }`).
fn entries_table<'a>(
    doc: &'a mut DocumentMut,
    key: &str,
    path: &Path,
) -> Result<(&'a mut dyn TableLike, bool), ConfigError> {
    let item = doc.as_table_mut().entry(key).or_insert_with(|| {
        let mut table = Table::new();
        table.set_implicit(true);
        Item::Table(table)
    });
    let inline = item.is_inline_table();
    let table = item
        .as_table_like_mut()
        .ok_or_else(|| validation(path, format!("{key} is not a table")))?;
    Ok((table, inline))
}

/// Write the script's fields, skipping values equal to `old` as
/// [`set_snippet_fields`] does.
fn set_script_fields(table: &mut dyn TableLike, script: &ScriptConfig, old: Option<&ScriptConfig>) {
    if old.is_none_or(|old| old.name != script.name) {
        set_value(table, "name", Value::from(script.name.as_str()));
    }
    if old.is_none_or(|old| old.path != script.path) {
        set_optional(table, "path", script.path.as_deref());
    }
    // Optional keys a tracked file does not need are left out when empty.
    fn nonempty(s: &str) -> Option<&str> {
        (!s.is_empty()).then_some(s)
    }
    if old.is_none_or(|old| old.interpreter != script.interpreter) {
        set_optional(table, "interpreter", nonempty(&script.interpreter));
    }
    if old.is_none_or(|old| old.args != script.args) {
        if script.args.is_empty() && script.path.is_some() {
            table.remove("args");
        } else {
            let args: Array = script.args.iter().map(String::as_str).collect();
            set_value(table, "args", Value::Array(args));
        }
    }
    if old.is_none_or(|old| old.cwd != script.cwd) {
        set_optional(table, "cwd", nonempty(&script.cwd));
    }
    if old.is_none_or(|old| old.hotkey != script.hotkey) {
        set_optional(table, "hotkey", script.hotkey.as_deref());
    }
}

/// Write `key` as a string, or remove it for `None`.
fn set_optional(table: &mut dyn TableLike, key: &str, value: Option<&str>) {
    match value {
        Some(value) => set_value(table, key, Value::from(value)),
        None => {
            table.remove(key);
        }
    }
}

/// Write the snippet's fields, skipping values equal to `old` so their original
/// spelling survives. A replaced value keeps its whitespace and comment.
fn set_snippet_fields(
    table: &mut dyn TableLike,
    snippet: &SnippetConfig,
    old: Option<&SnippetConfig>,
) {
    if old.is_none_or(|old| old.name != snippet.name) {
        set_value(table, "name", Value::from(snippet.name.as_str()));
    }
    if old.is_none_or(|old| old.keyword != snippet.keyword) {
        set_optional(table, "keyword", snippet.keyword.as_deref());
    }
    if old.is_none_or(|old| old.text != snippet.text) {
        set_value(table, "text", text_value(&snippet.text));
    }
}

fn set_value(table: &mut dyn TableLike, key: &str, value: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(slot) => {
            let decor = slot.decor().clone();
            *slot = value;
            *slot.decor_mut() = decor;
        }
        None => {
            table.insert(key, Item::Value(value));
        }
    }
}

/// Multi-line text reads best as a `'''` literal block, which TOML keeps
/// verbatim (the newline after the opening quotes is dropped). Text a literal
/// cannot hold falls back to an escaped string.
fn text_value(text: &str) -> Value {
    let literal = text.contains('\n')
        && !text.contains("'''")
        && !text.ends_with('\'')
        && !text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t');
    if literal && let Ok(value) = format!("'''\n{text}'''").parse::<Value>() {
        return value;
    }
    Value::from(text)
}

fn to_table(settings: &Settings, path: &Path) -> Result<toml::Table, ConfigError> {
    toml::Table::try_from(settings).map_err(|e| validation(path, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{BASE_SETTINGS, ClipboardSource, PaletteSize};

    const PATH: &str = "config.toml";

    const COMMENTED: &str = "\
# my smolcast config
schema_version = 1

[settings]
summon_hotkey = \"control+alt+Space\" # muscle memory
clipboard_capture = true
clipboard_max_entries = 100
clipboard_max_item_bytes = 8388608
clipboard_max_total_bytes = 67108864

[scripts.hello]
name = \"Hello\"   # aligned on purpose
interpreter = \"pwsh.exe\"
args = [\"-c\", \"echo hi\"]
cwd = \"C:\\\\\"

[snippets.sig]
name = \"Signature\" # mine
keyword = \";sig\"
text = '''
Best,
Me'''
";

    fn edit(existing: Option<&str>, settings: &Settings) -> Result<String, ConfigError> {
        write_settings(existing, settings, Path::new(PATH))
    }

    fn current(text: &str) -> ConfigFile {
        parse_str(text, Path::new(PATH)).unwrap()
    }

    #[test]
    fn unchanged_settings_leave_the_file_identical() {
        let settings = current(COMMENTED).settings;
        assert_eq!(edit(Some(COMMENTED), &settings).unwrap(), COMMENTED);
    }

    #[test]
    fn changes_only_edited_keys_and_keeps_comments_scripts_and_snippets() {
        let mut settings = current(COMMENTED).settings;
        settings.summon_hotkey = "control+shift+KeyK".into();
        settings.clipboard_max_entries = 250;
        let text = edit(Some(COMMENTED), &settings).unwrap();
        assert!(text.contains("# my smolcast config"));
        assert!(text.contains("summon_hotkey = \"control+shift+KeyK\" # muscle memory"));
        assert!(text.contains("clipboard_max_entries = 250\n"));
        assert!(text.contains("name = \"Hello\"   # aligned on purpose"));
        let parsed = current(&text);
        assert_eq!(parsed.settings, settings);
        assert!(parsed.scripts.contains_key("hello"));
        assert!(text.contains("text = '''\nBest,\nMe'''\n"));
        assert_eq!(parsed.snippets["sig"].text, "Best,\nMe");
    }

    #[test]
    fn optional_keys_are_added_only_when_they_differ() {
        let mut settings = current(COMMENTED).settings;
        settings.start_at_login = true;
        let text = edit(Some(COMMENTED), &settings).unwrap();
        assert!(text.contains("start_at_login = true"));
        assert!(!text.contains("clipboard_source"));
        assert!(!text.contains("clipboard_persist"));

        assert!(!text.contains("palette_size"));

        settings.clipboard_source = ClipboardSource::Smolcast;
        let text = edit(Some(&text), &settings).unwrap();
        assert!(text.contains("clipboard_source = \"smolcast\""));
        assert_eq!(current(&text).settings, settings);

        settings.palette_size = PaletteSize::Large;
        let text = edit(Some(&text), &settings).unwrap();
        assert!(text.contains("palette_size = \"large\""));
        assert_eq!(current(&text).settings, settings);
    }

    #[test]
    fn absent_file_is_created_valid() {
        let settings = Settings {
            clipboard_persist: false,
            ..Settings::default()
        };
        let text = edit(None, &settings).unwrap();
        assert!(text.starts_with("schema_version = 1\n\n[settings]\nsummon_hotkey = "));
        let parsed = current(&text);
        assert_eq!(parsed.schema_version, SCHEMA_VERSION);
        assert_eq!(parsed.settings, settings);
        assert!(parsed.scripts.is_empty());
    }

    #[test]
    fn refuses_invalid_or_newer_files() {
        let settings = Settings::default();
        assert!(matches!(
            edit(Some("schema_version = 1\n[settings\n"), &settings),
            Err(ConfigError::Parse { .. })
        ));
        let newer = COMMENTED.replace("schema_version = 1", "schema_version = 2");
        assert!(matches!(
            edit(Some(&newer), &settings),
            Err(ConfigError::Validation { .. })
        ));
    }

    #[test]
    fn refuses_invalid_settings() {
        let mut settings = current(COMMENTED).settings;
        settings.clipboard_max_entries = 0;
        assert!(matches!(
            edit(Some(COMMENTED), &settings),
            Err(ConfigError::Validation { .. })
        ));
    }

    fn snip(name: &str, keyword: Option<&str>, text: &str) -> SnippetConfig {
        SnippetConfig {
            name: name.into(),
            keyword: keyword.map(Into::into),
            text: text.into(),
        }
    }

    fn put(
        existing: Option<&str>,
        id: &str,
        snippet: SnippetConfig,
        create: bool,
    ) -> Result<String, ConfigError> {
        let edit = SnippetEdit::Put {
            id: id.into(),
            snippet,
            create,
        };
        write_snippet(existing, &edit, Path::new(PATH))
    }

    #[test]
    fn creating_a_snippet_appends_a_readable_table_and_keeps_the_rest() {
        let text = put(
            Some(COMMENTED),
            "addr",
            snip("Address", None, "221B\nBaker St"),
            true,
        )
        .unwrap();
        assert!(text.starts_with(COMMENTED), "{text}");
        assert!(
            text.ends_with(
                "\n[snippets.addr]\nname = \"Address\"\ntext = '''\n221B\nBaker St'''\n"
            ),
            "{text}"
        );
        let parsed = current(&text);
        assert_eq!(
            parsed.snippets["addr"],
            snip("Address", None, "221B\nBaker St")
        );
        assert_eq!(parsed.snippets["sig"].text, "Best,\nMe");
    }

    #[test]
    fn first_snippet_gets_no_bare_header_and_absent_file_is_created() {
        let text = put(
            Some(BASE_SETTINGS),
            "a",
            snip("A", Some(";a"), "one line"),
            true,
        )
        .unwrap();
        assert!(!text.contains("[snippets]\n"), "{text}");
        assert!(
            text.contains("[snippets.a]\nname = \"A\"\nkeyword = \";a\"\ntext = \"one line\"\n"),
            "{text}"
        );

        let created = put(None, "a", snip("A", None, "x"), true).unwrap();
        let parsed = current(&created);
        assert_eq!(parsed.settings, Settings::default());
        assert_eq!(parsed.snippets["a"], snip("A", None, "x"));
    }

    #[test]
    fn updating_rewrites_only_changed_fields() {
        let text = put(
            Some(COMMENTED),
            "sig",
            snip("Signature", Some(";s"), "Best,\nMe"),
            false,
        )
        .unwrap();
        assert_eq!(
            text,
            COMMENTED.replace("keyword = \";sig\"", "keyword = \";s\"")
        );

        let renamed = put(
            Some(COMMENTED),
            "sig",
            snip("Sign-off", None, "Best,\nMe"),
            false,
        )
        .unwrap();
        assert!(
            renamed.contains("name = \"Sign-off\" # mine\n"),
            "{renamed}"
        );
        assert!(!renamed.contains("keyword"), "{renamed}");
        assert!(renamed.contains("text = '''\nBest,\nMe'''\n"), "{renamed}");
    }

    #[test]
    fn deleting_removes_only_that_table() {
        let with_two = put(Some(COMMENTED), "addr", snip("Address", None, "x"), true).unwrap();
        let edit = SnippetEdit::Delete { id: "sig".into() };
        let text = write_snippet(Some(&with_two), &edit, Path::new(PATH)).unwrap();
        let parsed = current(&text);
        assert!(!parsed.snippets.contains_key("sig"));
        assert!(parsed.snippets.contains_key("addr"));
        assert!(text.contains("name = \"Hello\"   # aligned on purpose"));
    }

    #[test]
    fn stale_or_conflicting_snippet_edits_are_refused() {
        let validation =
            |r: Result<String, ConfigError>| matches!(r, Err(ConfigError::Validation { .. }));
        assert!(validation(put(
            Some(COMMENTED),
            "sig",
            snip("X", None, "x"),
            true
        )));
        assert!(validation(put(
            Some(COMMENTED),
            "nope",
            snip("X", None, "x"),
            false
        )));
        assert!(validation(write_snippet(
            Some(COMMENTED),
            &SnippetEdit::Delete { id: "nope".into() },
            Path::new(PATH)
        )));
        // Keyword collisions ignore case.
        assert!(validation(put(
            Some(COMMENTED),
            "b",
            snip("B", Some(";SIG"), "x"),
            true
        )));
        assert!(validation(put(
            Some(COMMENTED),
            "b",
            snip("", None, "x"),
            true
        )));
        assert!(matches!(
            put(
                Some("schema_version = 1\n[settings\n"),
                "b",
                snip("B", None, "x"),
                true
            ),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn text_a_literal_cannot_hold_still_round_trips() {
        for text in [
            "a '''b''' c\nd",
            "ends in a quote'\nx",
            "cr\r\nlf\n",
            "bell\u{7}\nx",
            "tab\tand \"quotes\" \\",
        ] {
            let written = put(Some(COMMENTED), "t", snip("T", None, text), true).unwrap();
            assert_eq!(current(&written).snippets["t"].text, text, "{written}");
        }
    }

    #[test]
    fn inline_snippets_table_stays_inline() {
        let text = "schema_version = 1\nsnippets = { x = { name = \"X\", text = \"x\" } }\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n";
        let written = put(Some(text), "y", snip("Y", None, "y"), true).unwrap();
        let parsed = current(&written);
        assert_eq!(parsed.snippets.len(), 2);
        assert!(
            written.contains("y = { name = \"Y\", text = \"y\" }"),
            "{written}"
        );
    }
}
