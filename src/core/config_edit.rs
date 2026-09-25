//! Settings-form writes to `config.toml` (plan §12.7). The file stays the single
//! source of truth: the form edits only the `[settings]` keys whose values
//! changed, preserving comments, formatting, key order, and `[scripts]`, and
//! the result is re-validated before the caller writes it.
//!
//! The §10.2 rules hold: an existing file that does not parse or validate
//! (including a newer `schema_version`) is never overwritten, and an absent
//! file is created only by an explicit save.

use std::path::Path;

use toml_edit::{DocumentMut, Item, Value};

use super::config::{ConfigError, ConfigFile, SCHEMA_VERSION, Settings, parse_str};

/// Return the new file text with `settings` applied. `existing` is the current
/// file text, or `None` when the file is absent.
pub fn write_settings(
    existing: Option<&str>,
    settings: &Settings,
    path: &Path,
) -> Result<String, ConfigError> {
    let invalid = |message: String| ConfigError::Validation {
        path: path.to_owned(),
        message,
    };
    settings.validate(path)?;

    let (mut doc, current) = match existing {
        Some(text) => {
            // Refuses malformed and newer-schema files before touching them.
            let current = parse_str(text, path)?.settings;
            let doc = text
                .parse::<DocumentMut>()
                .map_err(|e| invalid(e.to_string()))?;
            (doc, Some(current))
        }
        None => {
            // Serializing the struct keeps its declaration order.
            let body = toml::to_string(settings).map_err(|e| invalid(e.to_string()))?;
            let doc = format!("schema_version = {SCHEMA_VERSION}\n\n[settings]\n{body}")
                .parse::<DocumentMut>()
                .map_err(|e| invalid(e.to_string()))?;
            (doc, None)
        }
    };

    let new = to_table(settings, path)?;
    let old = match &current {
        Some(current) => Some(to_table(current, path)?),
        None => None,
    };
    let table = doc["settings"]
        .as_table_mut()
        .ok_or_else(|| invalid("[settings] is not a table".into()))?;
    for (key, value) in &new {
        if old.as_ref().and_then(|old| old.get(key)) == Some(value) {
            continue;
        }
        let replacement = value
            .to_string()
            .parse::<Value>()
            .map_err(|e| invalid(e.to_string()))?;
        match table.get_mut(key).and_then(Item::as_value_mut) {
            // Keep the key's surrounding whitespace and trailing comment.
            Some(slot) => {
                let decor = slot.decor().clone();
                *slot = replacement;
                *slot.decor_mut() = decor;
            }
            None => {
                table.insert(key, Item::Value(replacement));
            }
        }
    }

    let text = doc.to_string();
    let written: ConfigFile = parse_str(&text, path)?;
    if &written.settings != settings {
        return Err(invalid("edited file did not round-trip".into()));
    }
    Ok(text)
}

fn to_table(settings: &Settings, path: &Path) -> Result<toml::Table, ConfigError> {
    toml::Table::try_from(settings).map_err(|e| ConfigError::Validation {
        path: path.to_owned(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::ClipboardSource;

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
    fn changes_only_edited_keys_and_keeps_comments_and_scripts() {
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
    }

    #[test]
    fn optional_keys_are_added_only_when_they_differ() {
        let mut settings = current(COMMENTED).settings;
        settings.start_at_login = true;
        let text = edit(Some(COMMENTED), &settings).unwrap();
        assert!(text.contains("start_at_login = true"));
        assert!(!text.contains("clipboard_source"));
        assert!(!text.contains("clipboard_persist"));

        settings.clipboard_source = ClipboardSource::Smolcast;
        let text = edit(Some(&text), &settings).unwrap();
        assert!(text.contains("clipboard_source = \"smolcast\""));
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
}
