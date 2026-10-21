//! Config schema v1: settings plus named scripts and snippets, with pure validation.
//!
//! Windows-free and host-testable. Hotkey *strings* are validated at Windows
//! runtime (the `global-hotkey` crate is a Windows-only dependency), so this
//! module validates everything else and stops at the string for `hotkey`.
//!
//! Contract (see `docs/systemPatterns.md` §10.2):
//! - An absent file materializes `ConfigFile::default()` without creating a file.
//! - A present file must declare every settings field; unknown fields are
//!   rejected, and a malformed file is never overwritten by the app.
//! - `scripts` and `snippets` default to empty maps when their tables are absent.

use std::collections::BTreeMap;
use std::io;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use crate::core::web::SearchEngine;

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

/// §10.6 snippet validation limits.
pub const MAX_SNIPPET_NAME_CHARS: usize = 128;
pub const MAX_SNIPPET_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_SNIPPET_KEYWORD_CHARS: usize = 32;

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
    #[serde(default)]
    pub snippets: BTreeMap<String, SnippetConfig>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            settings: Settings::default(),
            scripts: BTreeMap::new(),
            snippets: BTreeMap::new(),
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
    /// Register smolcast in the per-user Run key (§12.7). Optional; off.
    #[serde(default)]
    pub start_at_login: bool,
    /// Window size: a larger palette is wider and shows more rows. Optional;
    /// medium.
    #[serde(default)]
    pub palette_size: PaletteSize,
    /// Font sizes, independent of the window size. Optional; medium.
    #[serde(default)]
    pub text_size: TextSize,
    /// Animations: follow Windows' "Animation effects", or force reduced
    /// motion on or off. Optional; follows Windows.
    #[serde(default)]
    pub reduce_motion: ReduceMotion,
    /// Seconds a dismissed palette keeps its view, query, and drafts; 0
    /// always starts fresh. Optional; 30.
    #[serde(default = "default_keep_state_secs")]
    pub keep_state_secs: u32,
    /// Engine for the "search the web" fallback. Optional; Google.
    #[serde(default)]
    pub search_engine: SearchEngine,
    /// URL template for `search_engine = "custom"`; `%s` is the query.
    /// Optional.
    #[serde(default = "default_web_search_url")]
    pub web_search_url: String,
}

fn default_web_search_url() -> String {
    crate::core::web::DEFAULT_SEARCH_URL.to_owned()
}

pub const DEFAULT_KEEP_STATE_SECS: u32 = 30;
pub const MAX_KEEP_STATE_SECS: u32 = 3600;

fn default_keep_state_secs() -> u32 {
    DEFAULT_KEEP_STATE_SECS
}

/// The user's preferred palette window size. The app maps it to window
/// dimensions and list row counts (`theme::window_size`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaletteSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl PaletteSize {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];
}

/// The user's preferred text size. The app maps it to font sizes
/// (`theme::install`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl TextSize {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];
}

/// Whether to reduce motion: fades stay, movement and scaling go
/// (`core::motion`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReduceMotion {
    /// Follow Windows' "Animation effects" setting.
    #[default]
    System,
    On,
    Off,
}

impl ReduceMotion {
    pub const ALL: [Self; 3] = [Self::System, Self::On, Self::Off];

    /// Resolve against Windows' "Animation effects" switch.
    pub fn reduced(self, system_animations: bool) -> bool {
        match self {
            Self::System => !system_animations,
            Self::On => true,
            Self::Off => false,
        }
    }
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
            start_at_login: false,
            palette_size: PaletteSize::Medium,
            text_size: TextSize::Medium,
            reduce_motion: ReduceMotion::System,
            keep_state_secs: DEFAULT_KEEP_STATE_SECS,
            search_engine: SearchEngine::Google,
            web_search_url: default_web_search_url(),
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

/// Saved text pasted from the palette (§10.6). The text is literal: nothing
/// in it is expanded or substituted.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SnippetConfig {
    pub name: String,
    pub text: String,
    /// Short searchable alias such as `;sig`. Reserved as the trigger for
    /// future as-you-type expansion, so it is validated strictly now.
    pub keyword: Option<String>,
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
    parse_str(text, path)
}

/// Parse and validate config text. `path` is used only for error reporting.
pub fn parse_str(text: &str, path: &Path) -> Result<ConfigFile, ConfigError> {
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
            validate_id(path, "script", id)?;
            script.validate(path)?;
        }
        // Keywords are unique ignoring case, so a future trigger map is
        // unambiguous whatever case rule it adopts.
        let mut keywords: BTreeMap<String, &str> = BTreeMap::new();
        for (id, snippet) in &self.snippets {
            validate_id(path, "snippet", id)?;
            snippet.validate(path, id)?;
            if let Some(keyword) = &snippet.keyword
                && let Some(other) = keywords.insert(keyword.to_lowercase(), id)
            {
                return Err(validation(
                    path,
                    format!("snippets {other:?} and {id:?} share the keyword {keyword:?}"),
                ));
            }
        }
        Ok(())
    }
}

impl Settings {
    pub fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        check_range(
            path,
            "clipboard_max_entries",
            self.clipboard_max_entries,
            MIN_CLIPBOARD_ENTRIES..=MAX_CLIPBOARD_ENTRIES,
        )?;
        let item = self.clipboard_max_item_bytes;
        check_range(
            path,
            "clipboard_max_item_bytes",
            item,
            MIN_CLIPBOARD_ITEM_BYTES..=MAX_CLIPBOARD_ITEM_BYTES,
        )?;
        let total = self.clipboard_max_total_bytes;
        if total < item || total > MAX_CLIPBOARD_TOTAL_BYTES {
            return Err(validation(
                path,
                format!(
                    "clipboard_max_total_bytes {total} must be within {item}..={MAX_CLIPBOARD_TOTAL_BYTES}"
                ),
            ));
        }
        check_range(
            path,
            "keep_state_secs",
            self.keep_state_secs,
            0..=MAX_KEEP_STATE_SECS,
        )?;
        if let Some(problem) = crate::core::web::template_problem(&self.web_search_url) {
            return Err(validation(path, format!("web_search_url {problem}")));
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

impl SnippetConfig {
    pub fn validate(&self, path: &Path, id: &str) -> Result<(), ConfigError> {
        match self.problem() {
            Some(what) => Err(validation(path, format!("snippet {id:?}: {what}"))),
            None => Ok(()),
        }
    }

    /// The first rule this snippet breaks, phrased for a form field (the
    /// snippet editor shows it as-is). Keyword uniqueness is a whole-file rule
    /// checked by [`ConfigFile::validate`].
    pub fn problem(&self) -> Option<String> {
        if self.name.is_empty() {
            return Some("name must be nonempty".into());
        }
        if self.name.chars().count() > MAX_SNIPPET_NAME_CHARS {
            return Some(format!(
                "name exceeds {MAX_SNIPPET_NAME_CHARS} Unicode scalar values"
            ));
        }
        if self.text.is_empty() {
            return Some("text must be nonempty".into());
        }
        if self.text.len() > MAX_SNIPPET_TEXT_BYTES {
            return Some(format!("text exceeds {MAX_SNIPPET_TEXT_BYTES} UTF-8 bytes"));
        }
        if contains_nul(&self.name) || contains_nul(&self.text) {
            return Some("field contains a NUL byte".into());
        }
        if let Some(keyword) = &self.keyword {
            let chars = keyword.chars().count();
            if !(1..=MAX_SNIPPET_KEYWORD_CHARS).contains(&chars) {
                return Some(format!(
                    "keyword must be 1..={MAX_SNIPPET_KEYWORD_CHARS} Unicode scalar values"
                ));
            }
            if keyword.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Some("keyword must not contain whitespace or control characters".into());
            }
        }
        None
    }
}

/// Script and snippet table keys match ASCII `[A-Za-z0-9_-]{1,64}`.
pub fn is_valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn validate_id(path: &Path, kind: &str, id: &str) -> Result<(), ConfigError> {
    if is_valid_id(id) {
        Ok(())
    } else {
        Err(validation(
            path,
            format!("{kind} id {id:?} must match [A-Za-z0-9_-]{{1,64}}"),
        ))
    }
}

fn validation(path: &Path, message: String) -> ConfigError {
    ConfigError::Validation {
        path: path.to_owned(),
        message,
    }
}

/// Reject `value` unless it lies in `range`. The message names the setting.
fn check_range<T>(
    path: &Path,
    name: &str,
    value: T,
    range: RangeInclusive<T>,
) -> Result<(), ConfigError>
where
    T: PartialOrd + std::fmt::Display + Copy,
{
    if range.contains(&value) {
        return Ok(());
    }
    let (start, end) = (*range.start(), *range.end());
    Err(validation(
        path,
        format!("{name} {value} outside {start}..={end}"),
    ))
}

fn contains_nul(s: &str) -> bool {
    s.as_bytes().contains(&0)
}

/// Key cap names for a `global-hotkey` chord: `control+alt+KeyP` becomes
/// `Ctrl`, `Alt`, `P` (docs/DESIGN.md: key names written out).
pub fn key_names(chord: &str) -> Vec<String> {
    chord
        .split('+')
        .map(|part| {
            let named = match part.to_ascii_lowercase().as_str() {
                "control" | "ctrl" => "Ctrl",
                "alt" | "option" => "Alt",
                "shift" => "Shift",
                "super" | "meta" | "cmd" | "command" | "win" => "Win",
                _ => "",
            };
            if !named.is_empty() {
                return named.to_owned();
            }
            ["Key", "Digit"]
                .iter()
                .find_map(|prefix| part.strip_prefix(prefix).filter(|rest| rest.len() == 1))
                .unwrap_or(part)
                .to_owned()
        })
        .collect()
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

[snippets.signature]
name = "Email signature"
keyword = ";sig" # optional
text = """
Best,
Me"""
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A valid config with settings only; tests append their own tables.
    const BASE_SETTINGS: &str = "schema_version = 1\n[settings]\nsummon_hotkey = \"control+alt+Space\"\nclipboard_capture = true\nclipboard_max_entries = 100\nclipboard_max_item_bytes = 8388608\nclipboard_max_total_bytes = 67108864\n";

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
        let cfg = parse(BASE_SETTINGS).unwrap();
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
        let e = parse(&format!("{BASE_SETTINGS}mystery = 1\n")).unwrap_err();
        assert!(matches!(e, ConfigError::Parse { .. }));
    }

    #[test]
    fn invalid_script_id_rejected() {
        // A quoted dotted key is valid TOML but fails the script-id charset rule.
        let e = parse(&format!(
            "{BASE_SETTINGS}[scripts.\"bad.id\"]\nname = \"x\"\ninterpreter = \"exe\"\nargs = []\ncwd = \"C:\\\\x\"\n"
        ))
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
            let text = format!("{BASE_SETTINGS}[scripts.a]\n{script}\n");
            let e = parse(&text).unwrap_err();
            assert!(
                err_contains(&e, &format!("{field} must be nonempty")),
                "{field}: {e}"
            );
        }
    }

    #[test]
    fn nul_in_script_field_rejected() {
        let e = parse(&format!(
            "{BASE_SETTINGS}[scripts.a]\nname = \"x\"\ninterpreter = \"exe\"\nargs = [\"has\\u0000nul\"]\ncwd = \"C:\\\\x\"\n"
        ))
        .unwrap_err();
        assert!(err_contains(&e, "NUL"));
    }

    #[test]
    fn name_over_limit_rejected() {
        let long = "a".repeat(129);
        let e = parse(&format!(
            "{BASE_SETTINGS}[scripts.a]\nname = \"{long}\"\ninterpreter = \"exe\"\nargs = []\ncwd = \"C:\\\\x\"\n"
        ))
        .unwrap_err();
        assert!(err_contains(&e, "Unicode scalar values"));
    }

    #[test]
    fn too_many_args_rejected() {
        let args = vec!["a".to_string(); 65].join("\", \"");
        let e = parse(&format!(
            "{BASE_SETTINGS}[scripts.a]\nname = \"x\"\ninterpreter = \"exe\"\nargs = [\"{args}\"]\ncwd = \"C:\\\\x\"\n"
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
        let cfg = parse(&format!(
            "{BASE_SETTINGS}[scripts.a]\nname = \"x\"\ninterpreter = \"exe\"\nargs = []\ncwd = \"C:\\\\x\"\n"
        ))
        .unwrap();
        assert_eq!(cfg.scripts["a"].hotkey, None);
        assert_eq!(cfg.settings.summon_hotkey, "control+alt+Space");
    }

    #[test]
    fn clipboard_source_is_optional_and_validated() {
        let base = BASE_SETTINGS;
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
    fn start_at_login_is_optional_and_off() {
        let base = BASE_SETTINGS;
        assert!(!parse(base).unwrap().settings.start_at_login);
        let on = format!("{base}start_at_login = true\n");
        assert!(parse(&on).unwrap().settings.start_at_login);
    }

    #[test]
    fn palette_size_is_optional_and_validated() {
        let base = BASE_SETTINGS;
        assert_eq!(
            parse(base).unwrap().settings.palette_size,
            PaletteSize::Medium
        );
        for (value, expected) in [
            ("small", PaletteSize::Small),
            ("medium", PaletteSize::Medium),
            ("large", PaletteSize::Large),
        ] {
            let text = format!("{base}palette_size = \"{value}\"\n");
            assert_eq!(parse(&text).unwrap().settings.palette_size, expected);
        }
        assert!(parse(&format!("{base}palette_size = \"huge\"\n")).is_err());

        assert_eq!(parse(base).unwrap().settings.text_size, TextSize::Medium);
        let large = format!("{base}text_size = \"large\"\n");
        assert_eq!(parse(&large).unwrap().settings.text_size, TextSize::Large);
        assert!(parse(&format!("{base}text_size = \"Large\"\n")).is_err());

        let settings = parse(base).unwrap().settings;
        assert_eq!(settings.reduce_motion, ReduceMotion::System);
        for (value, expected) in [
            ("system", ReduceMotion::System),
            ("on", ReduceMotion::On),
            ("off", ReduceMotion::Off),
        ] {
            let text = format!("{base}reduce_motion = \"{value}\"\n");
            assert_eq!(parse(&text).unwrap().settings.reduce_motion, expected);
        }
        assert!(parse(&format!("{base}reduce_motion = \"yes\"\n")).is_err());
        assert!(ReduceMotion::System.reduced(false));
        assert!(!ReduceMotion::System.reduced(true));
        assert!(ReduceMotion::On.reduced(true));
        assert!(!ReduceMotion::Off.reduced(false));
    }

    #[test]
    fn keep_state_secs_is_optional_and_bounded() {
        let base = BASE_SETTINGS;
        assert_eq!(parse(base).unwrap().settings.keep_state_secs, 30);
        let off = format!("{base}keep_state_secs = 0\n");
        assert_eq!(parse(&off).unwrap().settings.keep_state_secs, 0);
        assert!(parse(&format!("{base}keep_state_secs = 3601\n")).is_err());
        assert!(parse(&format!("{base}keep_state_secs = -1\n")).is_err());
    }

    #[test]
    fn web_search_url_is_optional_and_validated() {
        let base = BASE_SETTINGS;
        assert_eq!(
            parse(base).unwrap().settings.web_search_url,
            crate::core::web::DEFAULT_SEARCH_URL
        );
        let ddg = format!("{base}web_search_url = \"https://duckduckgo.com/?q=%s\"\n");
        assert!(parse(&ddg).is_ok());
        let no_query = format!("{base}web_search_url = \"https://duckduckgo.com/\"\n");
        assert!(parse(&no_query).is_err());

        assert_eq!(
            parse(base).unwrap().settings.search_engine,
            SearchEngine::Google
        );
        let kagi = format!("{base}search_engine = \"kagi\"\n");
        assert_eq!(
            parse(&kagi).unwrap().settings.search_engine,
            SearchEngine::Kagi
        );
        let ddg = format!("{base}search_engine = \"duckduckgo\"\n");
        assert_eq!(
            parse(&ddg).unwrap().settings.search_engine,
            SearchEngine::DuckDuckGo
        );
        assert!(parse(&format!("{base}search_engine = \"altavista\"\n")).is_err());
    }

    #[test]
    fn sample_snippet_parses_and_table_is_optional() {
        let cfg = parse(SAMPLE_TOML).unwrap();
        let snippet = &cfg.snippets["signature"];
        assert_eq!(snippet.name, "Email signature");
        assert_eq!(snippet.keyword.as_deref(), Some(";sig"));
        // TOML trims the newline right after the opening quotes.
        assert_eq!(snippet.text, "Best,\nMe");
        assert!(parse(BASE_SETTINGS).unwrap().snippets.is_empty());
    }

    #[test]
    fn invalid_snippets_rejected() {
        let long_name = "n".repeat(MAX_SNIPPET_NAME_CHARS + 1);
        let long_text = "t".repeat(MAX_SNIPPET_TEXT_BYTES + 1);
        let long_keyword = "k".repeat(MAX_SNIPPET_KEYWORD_CHARS + 1);
        let cases: Vec<(String, &str)> = vec![
            (
                "[snippets.\"a.b\"]\nname = \"x\"\ntext = \"y\"\n".into(),
                "snippet id",
            ),
            (
                "[snippets.a]\nname = \"\"\ntext = \"y\"\n".into(),
                "name must be nonempty",
            ),
            (
                format!("[snippets.a]\nname = \"{long_name}\"\ntext = \"y\"\n"),
                "name exceeds",
            ),
            (
                "[snippets.a]\nname = \"x\"\ntext = \"\"\n".into(),
                "text must be nonempty",
            ),
            (
                format!("[snippets.a]\nname = \"x\"\ntext = \"{long_text}\"\n"),
                "text exceeds",
            ),
            (
                "[snippets.a]\nname = \"x\"\ntext = \"a\\u0000b\"\n".into(),
                "NUL",
            ),
            (
                "[snippets.a]\nname = \"x\"\ntext = \"y\"\nkeyword = \"\"\n".into(),
                "keyword must be",
            ),
            (
                format!("[snippets.a]\nname = \"x\"\ntext = \"y\"\nkeyword = \"{long_keyword}\"\n"),
                "keyword must be",
            ),
            (
                "[snippets.a]\nname = \"x\"\ntext = \"y\"\nkeyword = \"a b\"\n".into(),
                "whitespace",
            ),
            (
                "[snippets.a]\nname = \"x\"\ntext = \"y\"\nkeyword = \"a\\tb\"\n".into(),
                "whitespace",
            ),
        ];
        for (table, needle) in &cases {
            let e = parse(&format!("{BASE_SETTINGS}{table}")).unwrap_err();
            assert!(err_contains(&e, needle), "{table:?}: {e}");
        }
    }

    #[test]
    fn snippet_keywords_are_unique_ignoring_case() {
        let text = format!(
            "{BASE_SETTINGS}[snippets.a]\nname = \"A\"\ntext = \"1\"\nkeyword = \";Sig\"\n[snippets.b]\nname = \"B\"\ntext = \"2\"\nkeyword = \";sig\"\n"
        );
        let e = parse(&text).unwrap_err();
        assert!(err_contains(&e, "share the keyword"), "{e}");
        let distinct = text.replace("keyword = \";sig\"", "keyword = \";sig2\"");
        assert_eq!(parse(&distinct).unwrap().snippets.len(), 2);
    }

    #[test]
    fn unknown_snippet_field_rejected() {
        let text =
            format!("{BASE_SETTINGS}[snippets.a]\nname = \"x\"\ntext = \"y\"\nexpand = true\n");
        assert!(matches!(parse(&text), Err(ConfigError::Parse { .. })));
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

    #[test]
    fn key_names_are_written_out() {
        assert_eq!(key_names("control+alt+Space"), ["Ctrl", "Alt", "Space"]);
        assert_eq!(key_names("control+shift+KeyP"), ["Ctrl", "Shift", "P"]);
        assert_eq!(key_names("alt+Digit1"), ["Alt", "1"]);
        assert_eq!(key_names("alt+F5"), ["Alt", "F5"]);
    }
}
