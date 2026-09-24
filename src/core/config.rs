//! Config schema v1: settings plus named scripts, with pure validation.
//!
//! Windows-free and host-testable. Hotkey *strings* are validated at Windows
//! runtime (the `global-hotkey` crate is a Windows-only dependency), so this
//! module validates everything else and stops at the string for `hotkey`.
//!
//! Contract (see `plans/IMPLEMENTATION_PLAN.md` §10.2):
//! - An absent file materializes `ConfigFile::default()` without creating a file.
//! - A present file must declare every settings field; unknown fields are
//!   rejected, and a malformed file is never overwritten by the app.
//! - `scripts` defaults to an empty map when the `[scripts]` table is absent.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

pub const DEFAULT_SUMMON_HOTKEY: &str = "control+alt+Space";
pub const DEFAULT_CLIPBOARD_MAX_ENTRIES: usize = 100;
pub const DEFAULT_CLIPBOARD_MAX_ITEM_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_CLIPBOARD_MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// §7.5 validation limits.
pub const MIN_CLIPBOARD_ENTRIES: usize = 1;
pub const MAX_CLIPBOARD_ENTRIES: usize = 500;
pub const MIN_CLIPBOARD_ITEM_BYTES: usize = 64 * 1024;
pub const MAX_CLIPBOARD_ITEM_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_CLIPBOARD_TOTAL_BYTES: usize = 512 * 1024 * 1024;

/// §10.2 script validation limits.
pub const MAX_SCRIPT_NAME_CHARS: usize = 128;
pub const MAX_SCRIPT_ARGS: usize = 64;
pub const MAX_SCRIPT_ARG_BYTES: usize = 32 * 1024;

/// A parsed and validated config document, or a typed reason it could not be
/// used. The caller chooses the recovery story (safe defaults + visible notice).
#[derive(Debug)]
pub enum ConfigError {
    /// The file could not be read for a reason other than "not found".
    Io { path: PathBuf, source: io::Error },
    /// The file bytes are not valid UTF-8.
    NotUtf8 { path: PathBuf },
    /// TOML or schema-shape decoding failed (includes line/column info in the message).
    Parse { path: PathBuf, message: String },
    /// The document decoded but violated a semantic rule.
    Validation { path: PathBuf, message: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io { path, source } => write!(f, "read {}: {source}", path.display()),
            ConfigError::NotUtf8 { path } => {
                write!(f, "config {} is not valid UTF-8", path.display())
            }
            ConfigError::Parse { path, message } => {
                write!(f, "parse {}: {message}", path.display())
            }
            ConfigError::Validation { path, message } => {
                write!(f, "invalid {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub schema_version: u32,
    pub settings: Settings,
    #[serde(default)]
    pub scripts: BTreeMap<String, ScriptConfig>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            settings: Settings::default(),
            scripts: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub summon_hotkey: String,
    pub clipboard_capture: bool,
    pub clipboard_max_entries: usize,
    pub clipboard_max_item_bytes: usize,
    pub clipboard_max_total_bytes: usize,
    /// Optional so configs written before it existed stay valid (§7.0).
    #[serde(default)]
    pub clipboard_source: ClipboardSource,
    /// Keep smolcast's own history across restarts, DPAPI-encrypted (§7.2).
    /// Optional for the same reason as `clipboard_source`.
    #[serde(default = "default_true")]
    pub clipboard_persist: bool,
}

fn default_true() -> bool {
    true
}

/// Which history backs the Clipboard History view (§7.0).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardSource {
    /// Windows clipboard history when it is enabled, smolcast's otherwise.
    #[default]
    Auto,
    Windows,
    Smolcast,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            summon_hotkey: DEFAULT_SUMMON_HOTKEY.to_owned(),
            clipboard_capture: true,
            clipboard_max_entries: DEFAULT_CLIPBOARD_MAX_ENTRIES,
            clipboard_max_item_bytes: DEFAULT_CLIPBOARD_MAX_ITEM_BYTES,
            clipboard_max_total_bytes: DEFAULT_CLIPBOARD_MAX_TOTAL_BYTES,
            clipboard_source: ClipboardSource::Auto,
            clipboard_persist: true,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScriptConfig {
    pub name: String,
    pub interpreter: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub hotkey: Option<String>,
}

/// Read and validate a config file (§10.2).
///
/// An absent path yields `ConfigFile::default()` and never creates the file.
pub fn load(path: &Path) -> Result<ConfigFile, ConfigError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(ConfigFile::default()),
        Err(source) => {
            return Err(ConfigError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| ConfigError::NotUtf8 {
        path: path.to_owned(),
    })?;
    let parsed: ConfigFile = toml::from_str(text).map_err(|e| ConfigError::Parse {
        path: path.to_owned(),
        message: e.to_string(),
    })?;
    parsed.validate(path)?;
    Ok(parsed)
}

impl ConfigFile {
    /// Validate every field against the schema-v1 rules. The `path` parameter is
    /// used only for error reporting.
    pub fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(validation(
                path,
                format!(
                    "unsupported schema_version {} (expected {SCHEMA_VERSION})",
                    self.schema_version
                ),
            ));
        }
        self.settings.validate(path)?;
        for (id, script) in &self.scripts {
            validate_script_id(path, id)?;
            script.validate(path)?;
        }
        Ok(())
    }
}

impl Settings {
    pub fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        let entries = self.clipboard_max_entries;
        if !(MIN_CLIPBOARD_ENTRIES..=MAX_CLIPBOARD_ENTRIES).contains(&entries) {
            return Err(validation(
                path,
                format!(
                    "clipboard_max_entries {entries} outside {MIN_CLIPBOARD_ENTRIES}..={MAX_CLIPBOARD_ENTRIES}"
                ),
            ));
        }
        let item = self.clipboard_max_item_bytes;
        if !(MIN_CLIPBOARD_ITEM_BYTES..=MAX_CLIPBOARD_ITEM_BYTES).contains(&item) {
            return Err(validation(
                path,
                format!(
                    "clipboard_max_item_bytes {item} outside {MIN_CLIPBOARD_ITEM_BYTES}..={MAX_CLIPBOARD_ITEM_BYTES}"
                ),
            ));
        }
        let total = self.clipboard_max_total_bytes;
        if total < item || total > MAX_CLIPBOARD_TOTAL_BYTES {
            return Err(validation(
                path,
                format!(
                    "clipboard_max_total_bytes {total} must be within {item}..={MAX_CLIPBOARD_TOTAL_BYTES}"
                ),
            ));
        }
        Ok(())
    }
}

impl ScriptConfig {
    pub fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(validation(path, "script name must be nonempty".into()));
        }
        if self.name.chars().count() > MAX_SCRIPT_NAME_CHARS {
            return Err(validation(
                path,
                format!("script name exceeds {MAX_SCRIPT_NAME_CHARS} Unicode scalar values"),
            ));
        }
        if self.interpreter.is_empty() {
            return Err(validation(
                path,
                "script interpreter must be nonempty".into(),
            ));
        }
        if self.cwd.is_empty() {
            return Err(validation(path, "script cwd must be nonempty".into()));
        }
        if self.args.len() > MAX_SCRIPT_ARGS {
            return Err(validation(
                path,
                format!("script has more than {MAX_SCRIPT_ARGS} arguments"),
            ));
        }
        for arg in &self.args {
            if arg.len() > MAX_SCRIPT_ARG_BYTES {
                return Err(validation(
                    path,
                    format!("script argument exceeds {MAX_SCRIPT_ARG_BYTES} UTF-8 bytes"),
                ));
            }
            if contains_nul(arg) {
                return Err(validation(
                    path,
                    "script argument contains a NUL byte".into(),
                ));
            }
        }
        for s in [&self.name, &self.interpreter, &self.cwd] {
            if contains_nul(s) {
                return Err(validation(path, "script field contains a NUL byte".into()));
            }
        }
        if self.hotkey.as_deref().is_some_and(contains_nul) {
            return Err(validation(path, "script hotkey contains a NUL byte".into()));
        }
        Ok(())
    }
}

fn validate_script_id(path: &Path, id: &str) -> Result<(), ConfigError> {
    let valid_len = (1..=64).contains(&id.len());
    let valid_chars = id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if valid_len && valid_chars {
        Ok(())
    } else {
        Err(validation(
            path,
            format!("script id {id:?} must match [A-Za-z0-9_-]{{1,64}}"),
        ))
    }
}

fn validation(path: &Path, message: String) -> ConfigError {
    ConfigError::Validation {
        path: path.to_owned(),
        message,
    }
}

fn contains_nul(s: &str) -> bool {
    s.as_bytes().contains(&0)
}

/// The canonical config sample from the plan's §10.2, used by tests.
#[cfg(test)]
pub(crate) const SAMPLE_TOML: &str = r#"
schema_version = 1

[settings]
summon_hotkey = "control+alt+Space"
clipboard_capture = true
clipboard_max_entries = 100
clipboard_max_item_bytes = 8388608
clipboard_max_total_bytes = 67108864

[scripts.open-project]
name = "Open project"
interpreter = "pwsh.exe"
args = ["-NoLogo", "-NoProfile", "-File", "C:\\Tools\\open-project.ps1"]
cwd = "C:\\Users\\me\\Projects"
hotkey = "control+alt+KeyP" # optional
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse(text: &str) -> Result<ConfigFile, ConfigError> {
        let cfg = toml::from_str::<ConfigFile>(text).map_err(|e| ConfigError::Parse {
            path: PathBuf::from("test.toml"),
            message: e.to_string(),
        })?;
        cfg.validate(Path::new("test.toml"))?;
        Ok(cfg)
    }

    fn err_contains(e: &ConfigError, needle: &str) -> bool {
        match e {
            ConfigError::Io { .. } | ConfigError::NotUtf8 { .. } => false,
            ConfigError::Parse { message, .. } | ConfigError::Validation { message, .. } => {
                message.contains(needle)
            }
        }
    }

    #[test]
    fn absent_file_materializes_defaults_without_creating_it() {
        let dir = std::env::temp_dir().join(format!("smolcast-config-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.settings.summon_hotkey, "control+alt+Space");
        assert!(loaded.settings.clipboard_capture);
        assert_eq!(loaded.settings.clipboard_max_entries, 100);
        assert_eq!(loaded.settings.clipboard_max_item_bytes, 8 * 1024 * 1024);
        assert_eq!(loaded.settings.clipboard_max_total_bytes, 64 * 1024 * 1024);
        assert!(loaded.scripts.is_empty());

        assert!(!path.exists(), "absent config must not be created");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn canonical_sample_parses() {
        let cfg = parse(SAMPLE_TOML).unwrap();
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(cfg.settings.summon_hotkey, "control+alt+Space");
        let script = &cfg.scripts["open-project"];
        assert_eq!(script.name, "Open project");
        assert_eq!(script.interpreter, "pwsh.exe");
        assert_eq!(script.args.len(), 4);
        assert_eq!(script.cwd, "C:\\Users\\me\\Projects");
        assert_eq!(script.hotkey.as_deref(), Some("control+alt+KeyP"));
    }

    #[test]
    fn scripts_table_is_optional() {
        let cfg = parse(
            "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n",
        )
        .unwrap();
        assert!(cfg.scripts.is_empty());
    }

    #[test]
    fn schema_version_mismatch_rejected() {
        let e = parse(
            "schema_version = 2\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n",
        )
        .unwrap_err();
        assert!(err_contains(&e, "schema_version"));
    }

    #[test]
    fn unknown_field_rejected() {
        let e = parse(
            "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\nmystery = 1\n",
        )
        .unwrap_err();
        assert!(matches!(e, ConfigError::Parse { .. }));
    }

    #[test]
    fn invalid_script_id_rejected() {
        // A quoted dotted key is valid TOML but fails the script-id charset rule.
        let e = parse(
            "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n[scripts.\"bad.id\"]\nname = \"x\"\ninterpreter = \"exe\"\nargs = []\ncwd = \"C:\\\\x\"\n",
        )
        .unwrap_err();
        assert!(err_contains(&e, "script id"));
    }

    #[test]
    fn empty_name_interpreter_cwd_rejected() {
        let mut texts = vec![
            (
                "name",
                "name = \"\"\ninterpreter = \"exe\"\nargs = []\ncwd = \"C:\\\\x\"",
            ),
            (
                "interpreter",
                "name = \"x\"\ninterpreter = \"\"\nargs = []\ncwd = \"C:\\\\x\"",
            ),
            (
                "cwd",
                "name = \"x\"\ninterpreter = \"exe\"\nargs = []\ncwd = \"\"",
            ),
        ];
        for (field, script) in texts.drain(..) {
            let text = format!(
                "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n[scripts.a]\n{script}\n"
            );
            let e = parse(&text).unwrap_err();
            assert!(
                err_contains(&e, &format!("{field} must be nonempty")),
                "{field}: {e}"
            );
        }
    }

    #[test]
    fn nul_in_script_field_rejected() {
        let e = parse(
            "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n[scripts.a]\nname = \"x\"\ninterpreter = \"exe\"\nargs = [\"has\\u0000nul\"]\ncwd = \"C:\\\\x\"\n",
        )
        .unwrap_err();
        assert!(err_contains(&e, "NUL"));
    }

    #[test]
    fn name_over_limit_rejected() {
        let long = "a".repeat(129);
        let e = parse(&format!(
            "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n[scripts.a]\nname = \"{long}\"\ninterpreter = \"exe\"\nargs = []\ncwd = \"C:\\\\x\"\n"
        ))
        .unwrap_err();
        assert!(err_contains(&e, "Unicode scalar values"));
    }

    #[test]
    fn too_many_args_rejected() {
        let args = vec!["a".to_string(); 65].join("\", \"");
        let e = parse(&format!(
            "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n[scripts.a]\nname = \"x\"\ninterpreter = \"exe\"\nargs = [\"{args}\"]\ncwd = \"C:\\\\x\"\n"
        ))
        .unwrap_err();
        assert!(err_contains(&e, "arguments"));
    }

    #[test]
    fn clipboard_limit_bounds_enforced() {
        let base_settings = |entries: usize, item: usize, total: usize| {
            format!(
                "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = {entries}\nclipboard_max_item_bytes = {item}\nclipboard_max_total_bytes = {total}\n"
            )
        };

        // Valid boundaries.
        parse(&base_settings(1, 64 * 1024, 64 * 1024)).unwrap();
        parse(&base_settings(500, 64 * 1024 * 1024, 512 * 1024 * 1024)).unwrap();

        // Out of range.
        let e = parse(&base_settings(0, 8 * 1024 * 1024, 64 * 1024 * 1024)).unwrap_err();
        assert!(err_contains(&e, "clipboard_max_entries"));
        let e = parse(&base_settings(501, 8 * 1024 * 1024, 64 * 1024 * 1024)).unwrap_err();
        assert!(err_contains(&e, "clipboard_max_entries"));
        let e = parse(&base_settings(100, 1024, 64 * 1024 * 1024)).unwrap_err();
        assert!(err_contains(&e, "clipboard_max_item_bytes"));
        let e = parse(&base_settings(100, 8 * 1024 * 1024, 4 * 1024 * 1024)).unwrap_err();
        assert!(err_contains(&e, "clipboard_max_total_bytes"));
        let e = parse(&base_settings(100, 8 * 1024 * 1024, 513 * 1024 * 1024)).unwrap_err();
        assert!(err_contains(&e, "clipboard_max_total_bytes"));
    }

    #[test]
    fn hotkey_is_optional_and_passthrough() {
        let cfg = parse(
            "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n[scripts.a]\nname = \"x\"\ninterpreter = \"exe\"\nargs = []\ncwd = \"C:\\\\x\"\n",
        )
        .unwrap();
        assert_eq!(cfg.scripts["a"].hotkey, None);
        assert_eq!(cfg.settings.summon_hotkey, "control+alt+Space");
    }

    #[test]
    fn clipboard_source_is_optional_and_validated() {
        let base = "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n";
        assert_eq!(
            parse(base).unwrap().settings.clipboard_source,
            ClipboardSource::Auto
        );
        for (value, expected) in [
            ("auto", ClipboardSource::Auto),
            ("windows", ClipboardSource::Windows),
            ("smolcast", ClipboardSource::Smolcast),
        ] {
            let text = format!("{base}clipboard_source = \"{value}\"\n");
            assert_eq!(parse(&text).unwrap().settings.clipboard_source, expected);
        }
        assert!(parse(base).unwrap().settings.clipboard_persist);
        let off = format!("{base}clipboard_persist = false\n");
        assert!(!parse(&off).unwrap().settings.clipboard_persist);
        let bad = format!("{base}clipboard_source = \"Windows\"\n");
        assert!(matches!(parse(&bad), Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn non_utf8_file_is_typed_error() {
        let dir = std::env::temp_dir().join(format!("smolcast-config-utf8-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, [0xff, 0xfe, 0x00, 0x41]).unwrap();

        let e = load(&path).unwrap_err();
        assert!(matches!(e, ConfigError::NotUtf8 { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
