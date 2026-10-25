//! Launcher index models: stable app identity, comparison keys, and
//! deterministic duplicate selection.
//!
//! Windows-free and host-testable. The Windows shell scanner (`win/shell.rs`)
//! resolves shortcuts and AppsFolder items into [`RawEntry`] values with
//! comparison keys already normalized by [`path_key`] / [`aumid_key`]; this
//! module owns identity and merging only.
//!
//! Contract (see `docs/systemPatterns.md` §6.1–§6.2):
//! - A shortcut carrying an AppUserModelID and an AppsFolder item with the same
//!   normalized AUMID share `AppId::Packaged`.
//! - Resolved classic shortcuts merge only when target, raw arguments, and
//!   working directory all match.
//! - An unresolved shortcut is keyed by its own `.lnk` path and never merges
//!   with a resolved entry.
//! - Duplicates keep one launch action chosen by source priority (current-user
//!   Start Menu, common Start Menu, AppsFolder, installed programs); discarded
//!   names become aliases.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::config::{QuicklinkConfig, ScriptConfig};
use crate::core::script_file;

/// Stable identity of one launch action; also the usage-state key (§10.4).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppId {
    Packaged {
        aumid_key: String,
    },
    Classic {
        target_key: String,
        arguments: String,
        cwd_key: String,
    },
    UnresolvedShortcut {
        path_key: String,
    },
    /// A built-in palette command (plan §5.4).
    Command {
        name: String,
    },
    /// A configured script, by its config table id (§10.3).
    Script {
        id: String,
    },
    /// A configured quicklink, by its config table id (§10.7).
    Quicklink {
        id: String,
    },
}

/// Where an entry was enumerated. Declaration order is selection priority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AppSource {
    UserStartMenu,
    CommonStartMenu,
    AppsFolder,
    /// An Uninstall registry row with no Start Menu shortcut (decision 027).
    InstalledPrograms,
    /// Built into smolcast; never deduplicated with enumerated apps.
    Builtin,
    /// A `[scripts.<id>]` or `[quicklinks.<id>]` entry in config.toml.
    Config,
}

/// How to launch an entry. Paths and AUMIDs keep their original case; the
/// lowercase comparison keys live in [`AppId`] only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchSpec {
    /// Launched by passing the `.lnk` itself to the shell so shortcut
    /// arguments and working directory survive.
    Shortcut { lnk_path: PathBuf },
    /// Launched through `IApplicationActivationManager::ActivateApplication`.
    Packaged { aumid: String },
    /// An installed program's executable, started by the shell in its own
    /// folder (decision 027).
    Executable { path: PathBuf },
    /// Opens a palette view instead of launching a process.
    Command(Command),
    /// Spawns a configured script (§10.3).
    Script(ScriptSpec),
    /// Runs a system action such as Lock Screen or Restart.
    System(SystemAction),
    /// Opens a URL, file or folder with the shell (§10.7). In the index it
    /// holds the quicklink's template; a launch request holds the expanded
    /// target.
    Quicklink { target: String },
}

/// The exact process a script runs: the interpreter executable, its argument
/// vector, and the working directory, from config (a tracked file's program
/// and folder resolved by `script_file::resolve`). Search text is never
/// appended, expanded, or substituted; only arguments typed into "Run with
/// Arguments" are appended, each as its own element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptSpec {
    pub interpreter: String,
    pub args: Vec<String>,
    pub cwd: String,
    /// The tracked file, checked to exist before the spawn: an interpreter
    /// given a missing file would start, fail silently, and look like a run.
    pub file: Option<String>,
}

/// The table id as an alias of a config entry, unless it already is the name.
fn id_alias(id: &str, name: &str) -> Vec<String> {
    if id.eq_ignore_ascii_case(name) {
        Vec::new()
    } else {
        vec![id.to_owned()]
    }
}

/// Root-list entries for configured scripts (§10.3). The script id is an
/// alias, so it is searchable alongside the display name.
pub fn script_entries(scripts: &BTreeMap<String, ScriptConfig>) -> Vec<IndexedApp> {
    scripts
        .iter()
        .map(|(id, script)| {
            let run = script_file::resolve(script, &[]);
            IndexedApp {
                id: AppId::Script { id: id.clone() },
                display_name: script.name.clone(),
                sort_name: script.name.to_lowercase(),
                aliases: id_alias(id, &script.name),
                source: AppSource::Config,
                // A tracked file shows its own icon (its type's, in
                // Explorer); a command shows its program's, and the icon
                // worker resolves bare names. Nothing on a network share is
                // read just to draw a row (no SMB/NTLM traffic on indexing).
                icon_key: match &script.path {
                    Some(path) if !script_file::is_unc(path) => path.clone(),
                    _ if script_file::is_unc(&run.interpreter) => String::new(),
                    _ => run.interpreter.clone(),
                },
                launch: LaunchSpec::Script(ScriptSpec {
                    interpreter: run.interpreter,
                    args: run.args,
                    cwd: run.cwd,
                    file: script.path.clone(),
                }),
            }
        })
        .collect()
}

/// Root-list entries for configured quicklinks (§10.7). The id is the
/// keyword that takes a query (`gh rust egui`), and an alias.
pub fn quicklink_entries(quicklinks: &BTreeMap<String, QuicklinkConfig>) -> Vec<IndexedApp> {
    quicklinks
        .iter()
        .map(|(id, link)| IndexedApp {
            id: AppId::Quicklink { id: id.clone() },
            display_name: link.name.clone(),
            sort_name: link.name.to_lowercase(),
            aliases: id_alias(id, &link.name),
            source: AppSource::Config,
            launch: LaunchSpec::Quicklink {
                target: link.url.clone(),
            },
            icon_key: String::new(),
        })
        .collect()
}

/// Built-in commands listed at the palette root (§5.4).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Command {
    ClipboardHistory,
    Emoji,
    Snippets,
    /// Opens the snippet view straight into a new-snippet form (§10.6).
    CreateSnippet,
    /// Lists tracked scripts to run, edit or untrack (§10.3).
    Scripts,
    /// Picks a script file on disk to track (§10.3).
    TrackScript,
    Settings,
    /// The version and credits.
    About,
    /// Opens the file search view (§6.6).
    SearchFiles,
    /// Asks GitHub for a newer release now (§13).
    CheckForUpdates,
    /// Opens a prefilled GitHub issue (§14).
    ReportBug,
    /// Lists hidden entries to unhide (§6.3).
    HiddenEntries,
}

impl Command {
    pub const ALL: [Command; 12] = [
        Command::ClipboardHistory,
        Command::Emoji,
        Command::Snippets,
        Command::CreateSnippet,
        Command::Scripts,
        Command::TrackScript,
        Command::Settings,
        Command::About,
        Command::SearchFiles,
        Command::CheckForUpdates,
        Command::ReportBug,
        Command::HiddenEntries,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Command::ClipboardHistory => "Clipboard History",
            Command::Emoji => "Search Emoji",
            Command::Snippets => "Search Snippets",
            Command::CreateSnippet => "Create Snippet",
            Command::Scripts => "Manage Scripts",
            Command::TrackScript => "Track Script File",
            Command::Settings => "smolcast settings",
            Command::About => "About smolcast",
            Command::SearchFiles => "Search Files",
            Command::CheckForUpdates => "Check for Updates",
            Command::ReportBug => "Report a Bug",
            Command::HiddenEntries => "Manage Hidden Entries",
        }
    }

    /// Other names a command is found by.
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            Command::Scripts => &["Scripts", "Edit Scripts"],
            Command::TrackScript => &["Add Script", "New Script", "Import Script"],
            Command::About => &["Credits"],
            Command::SearchFiles => &["Find Files", "Documents"],
            Command::CheckForUpdates => &["Update smolcast"],
            Command::ReportBug => &["Send Feedback", "File an Issue", "Bug Report"],
            Command::HiddenEntries => &["Unhide", "Hidden Apps", "Show Hidden"],
            _ => &[],
        }
    }

    /// Stable usage-state key.
    pub fn key(self) -> &'static str {
        match self {
            Command::ClipboardHistory => "clipboard-history",
            Command::Emoji => "emoji",
            Command::Snippets => "snippets",
            Command::CreateSnippet => "create-snippet",
            Command::Scripts => "scripts",
            Command::TrackScript => "track-script",
            Command::Settings => "settings",
            Command::About => "about",
            Command::SearchFiles => "search-files",
            Command::CheckForUpdates => "check-for-updates",
            Command::ReportBug => "report-bug",
            Command::HiddenEntries => "hidden-entries",
        }
    }

    pub fn id(self) -> AppId {
        AppId::Command {
            name: self.key().into(),
        }
    }
}

/// System actions listed at the palette root. They run on a shell job like
/// a launch; the ones that end the session or delete files ask for a second
/// Enter first.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SystemAction {
    LockScreen,
    Sleep,
    Restart,
    ShutDown,
    SignOut,
    EmptyRecycleBin,
    /// Switches Windows between light and dark mode.
    ToggleDarkMode,
    ToggleMute,
    TurnOffDisplay,
}

impl SystemAction {
    pub const ALL: [SystemAction; 9] = [
        SystemAction::LockScreen,
        SystemAction::Sleep,
        SystemAction::Restart,
        SystemAction::ShutDown,
        SystemAction::SignOut,
        SystemAction::EmptyRecycleBin,
        SystemAction::ToggleDarkMode,
        SystemAction::ToggleMute,
        SystemAction::TurnOffDisplay,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SystemAction::LockScreen => "Lock Screen",
            SystemAction::Sleep => "Sleep",
            SystemAction::Restart => "Restart",
            SystemAction::ShutDown => "Shut Down",
            SystemAction::SignOut => "Sign Out",
            SystemAction::EmptyRecycleBin => "Empty Recycle Bin",
            SystemAction::ToggleDarkMode => "Toggle Dark Mode",
            SystemAction::ToggleMute => "Toggle Mute",
            SystemAction::TurnOffDisplay => "Turn Off Display",
        }
    }

    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            SystemAction::LockScreen => &["Lock Computer"],
            SystemAction::Sleep => &["Suspend"],
            SystemAction::Restart => &["Reboot"],
            SystemAction::ShutDown => &["Power Off", "Turn Off Computer"],
            SystemAction::SignOut => &["Log Out", "Log Off"],
            SystemAction::EmptyRecycleBin => &["Empty Trash"],
            SystemAction::ToggleDarkMode => &["Light Mode", "Appearance"],
            SystemAction::ToggleMute => &["Mute", "Unmute", "Volume"],
            SystemAction::TurnOffDisplay => &["Screen Off", "Monitor Off"],
        }
    }

    /// Stable usage-state key.
    pub fn key(self) -> &'static str {
        match self {
            SystemAction::LockScreen => "system-lock-screen",
            SystemAction::Sleep => "system-sleep",
            SystemAction::Restart => "system-restart",
            SystemAction::ShutDown => "system-shut-down",
            SystemAction::SignOut => "system-sign-out",
            SystemAction::EmptyRecycleBin => "system-empty-recycle-bin",
            SystemAction::ToggleDarkMode => "system-toggle-dark-mode",
            SystemAction::ToggleMute => "system-toggle-mute",
            SystemAction::TurnOffDisplay => "system-turn-off-display",
        }
    }

    /// Whether a second Enter must confirm it: it closes every app or
    /// deletes files for good.
    pub fn confirms(self) -> bool {
        matches!(
            self,
            SystemAction::Restart
                | SystemAction::ShutDown
                | SystemAction::SignOut
                | SystemAction::EmptyRecycleBin
        )
    }

    pub fn id(self) -> AppId {
        AppId::Command {
            name: self.key().into(),
        }
    }
}

/// Root-list entries for every built-in command and system action.
/// `self_update` is false in the Microsoft Store build, which has no
/// "Check for Updates" (plan §15).
pub fn builtin_commands(self_update: bool) -> Vec<IndexedApp> {
    let commands = Command::ALL
        .into_iter()
        .filter(|&command| self_update || command != Command::CheckForUpdates)
        .map(|command| {
            builtin_entry(
                command.id(),
                command.title(),
                command.aliases(),
                LaunchSpec::Command(command),
            )
        });
    let actions = SystemAction::ALL.into_iter().map(|action| {
        builtin_entry(
            action.id(),
            action.title(),
            action.aliases(),
            LaunchSpec::System(action),
        )
    });
    commands.chain(actions).collect()
}

fn builtin_entry(id: AppId, title: &str, aliases: &[&str], launch: LaunchSpec) -> IndexedApp {
    IndexedApp {
        id,
        display_name: title.into(),
        sort_name: title.to_lowercase(),
        aliases: aliases.iter().map(|&a| a.to_owned()).collect(),
        source: AppSource::Builtin,
        launch,
        icon_key: String::new(),
    }
}

/// One enumerated row before deduplication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawEntry {
    pub id: AppId,
    pub display_name: String,
    pub source: AppSource,
    pub launch: LaunchSpec,
    /// Opaque key the icon extractor understands (a `.lnk` path or an AUMID).
    pub icon_key: String,
}

/// One searchable, launchable row after deduplication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedApp {
    pub id: AppId,
    pub display_name: String,
    /// `display_name` Unicode-lowercased once during indexing (ranking tie-break).
    pub sort_name: String,
    /// Display names of merged duplicates, sorted and deduplicated.
    pub aliases: Vec<String>,
    pub source: AppSource,
    pub launch: LaunchSpec,
    pub icon_key: String,
}

/// Identity comparison key for a Windows path: `/` becomes `\`, redundant `.`
/// components are removed, and the result is lowercased. Callers pass absolute
/// paths; the key is never used to open a file.
pub fn path_key(path: &str) -> String {
    let unified = path.replace('/', "\\");
    // Keep a UNC (`\\server`) or drive prefix intact; drop only `.` segments.
    let (prefix, rest) = match unified.strip_prefix("\\\\") {
        Some(rest) => ("\\\\", rest),
        None => ("", unified.as_str()),
    };
    let segments: Vec<&str> = rest.split('\\').filter(|s| *s != ".").collect();
    let mut key = String::with_capacity(unified.len());
    key.push_str(prefix);
    key.push_str(&segments.join("\\"));
    key.to_lowercase()
}

/// Identity comparison key for an AppUserModelID.
pub fn aumid_key(aumid: &str) -> String {
    aumid.to_lowercase()
}

/// True for an AppsFolder AUMID with package identity:
/// `<Name>_<13-char publisher id>!<AppId>`. Classic AppsFolder rows use a
/// shortcut-provided ID (`Brave`), an executable path, or a shell GUID instead.
pub fn is_packaged_aumid(aumid: &str) -> bool {
    let Some((family, app)) = aumid.split_once('!') else {
        return false;
    };
    let Some((name, publisher)) = family.rsplit_once('_') else {
        return false;
    };
    !name.is_empty()
        && !app.is_empty()
        && !app.contains('!')
        && !family.contains('\\')
        && publisher.len() == 13
        && publisher
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// What `IShellLinkW` resolved for a shortcut, in original case.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShortcutTarget {
    pub target: String,
    pub arguments: String,
    /// Empty when the shortcut sets none.
    pub working_dir: String,
}

/// Identity of a Start Menu shortcut (§6.2). A shortcut shares
/// `AppId::Packaged` only when its AppUserModelID names a packaged app that
/// AppsFolder also enumerated; any other AUMID (classic apps set their own)
/// must not merge distinct launch actions. Without a working directory the
/// effective one is the target's folder.
pub fn shortcut_id(
    lnk_path: &str,
    target: Option<&ShortcutTarget>,
    aumid: Option<&str>,
    packaged_aumid_keys: &BTreeSet<String>,
) -> AppId {
    if let Some(key) = aumid.map(aumid_key)
        && packaged_aumid_keys.contains(&key)
    {
        return AppId::Packaged { aumid_key: key };
    }
    match target {
        Some(t) if !t.target.is_empty() => {
            let cwd = if t.working_dir.is_empty() {
                parent_dir(&t.target)
            } else {
                t.working_dir.as_str()
            };
            AppId::Classic {
                target_key: path_key(&t.target),
                arguments: t.arguments.clone(),
                cwd_key: path_key(cwd),
            }
        }
        _ => AppId::UnresolvedShortcut {
            path_key: path_key(lnk_path),
        },
    }
}

/// The containing directory. A drive root keeps its separator (`C:\\`), so
/// an implicit cwd keys the same as an explicit one.
fn parent_dir(path: &str) -> &str {
    path.rfind(['\\', '/']).map_or("", |i| {
        let dir = &path[..i];
        if dir.ends_with(':') { &path[..=i] } else { dir }
    })
}

/// Merge entries that share an [`AppId`] (§6.2). The output is sorted by
/// `AppId`, so it never depends on filesystem or enumeration order.
pub fn dedup(entries: Vec<RawEntry>) -> Vec<IndexedApp> {
    let mut groups: BTreeMap<AppId, Vec<RawEntry>> = BTreeMap::new();
    for entry in entries {
        groups.entry(entry.id.clone()).or_default().push(entry);
    }

    groups
        .into_values()
        .map(|mut group| {
            // Source priority first; the remaining keys only make equal-priority
            // duplicates (two shortcuts in one tree) deterministic.
            group.sort_by(|a, b| {
                a.source
                    .cmp(&b.source)
                    .then_with(|| a.display_name.cmp(&b.display_name))
                    .then_with(|| launch_order_key(&a.launch).cmp(&launch_order_key(&b.launch)))
            });
            let mut members = group.into_iter();
            let chosen = members.next().expect("groups are never empty");
            let mut aliases: Vec<String> = members
                .map(|entry| entry.display_name)
                .filter(|name| *name != chosen.display_name)
                .collect();
            aliases.sort();
            aliases.dedup();
            IndexedApp {
                sort_name: chosen.display_name.to_lowercase(),
                id: chosen.id,
                display_name: chosen.display_name,
                aliases,
                source: chosen.source,
                launch: chosen.launch,
                icon_key: chosen.icon_key,
            }
        })
        .collect()
}

fn launch_order_key(launch: &LaunchSpec) -> String {
    match launch {
        LaunchSpec::Shortcut { lnk_path } => lnk_path.to_string_lossy().into_owned(),
        LaunchSpec::Packaged { aumid } => aumid.clone(),
        LaunchSpec::Executable { path } => path.to_string_lossy().into_owned(),
        LaunchSpec::Command(command) => command.key().into(),
        LaunchSpec::Script(spec) => spec.interpreter.clone(),
        LaunchSpec::System(action) => action.key().into(),
        LaunchSpec::Quicklink { target } => target.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(name: &str, source: AppSource, lnk: &str, id: AppId) -> RawEntry {
        RawEntry {
            id,
            display_name: name.into(),
            source,
            launch: LaunchSpec::Shortcut {
                lnk_path: PathBuf::from(lnk),
            },
            icon_key: lnk.into(),
        }
    }

    fn classic(target: &str, args: &str, cwd: &str) -> AppId {
        AppId::Classic {
            target_key: path_key(target),
            arguments: args.into(),
            cwd_key: path_key(cwd),
        }
    }

    #[test]
    fn path_key_normalizes_separators_dots_and_case() {
        assert_eq!(
            path_key("C:/Program Files/./App/APP.exe"),
            "c:\\program files\\app\\app.exe"
        );
        assert_eq!(path_key("\\\\Server\\Share\\.\\X"), "\\\\server\\share\\x");
        // `..` is not a redundant component and is preserved.
        assert_eq!(path_key("C:\\a\\..\\b"), "c:\\a\\..\\b");
    }

    #[test]
    fn duplicate_selection_follows_source_priority_and_unions_aliases() {
        let aumid = "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App";
        let packaged = AppId::Packaged {
            aumid_key: aumid_key(aumid),
        };
        let apps_folder = RawEntry {
            id: packaged.clone(),
            display_name: "Terminal".into(),
            source: AppSource::AppsFolder,
            launch: LaunchSpec::Packaged {
                aumid: aumid.into(),
            },
            icon_key: aumid.into(),
        };
        let common = shortcut(
            "Windows Terminal",
            AppSource::CommonStartMenu,
            "C:\\ProgramData\\Start\\Terminal.lnk",
            packaged.clone(),
        );
        let user = shortcut(
            "My Terminal",
            AppSource::UserStartMenu,
            "C:\\Users\\me\\Start\\Terminal.lnk",
            packaged.clone(),
        );
        // A second AppsFolder row with the chosen name must not become an alias.
        let mut same_name = apps_folder.clone();
        same_name.display_name = "My Terminal".into();

        for order in [
            vec![
                apps_folder.clone(),
                common.clone(),
                user.clone(),
                same_name.clone(),
            ],
            vec![
                same_name.clone(),
                user.clone(),
                apps_folder.clone(),
                common.clone(),
            ],
        ] {
            let apps = dedup(order);
            assert_eq!(apps.len(), 1);
            let app = &apps[0];
            assert_eq!(app.id, packaged);
            assert_eq!(app.source, AppSource::UserStartMenu);
            assert_eq!(app.display_name, "My Terminal");
            assert_eq!(app.sort_name, "my terminal");
            assert_eq!(app.aliases, vec!["Terminal", "Windows Terminal"]);
            assert_eq!(
                app.launch,
                LaunchSpec::Shortcut {
                    lnk_path: PathBuf::from("C:\\Users\\me\\Start\\Terminal.lnk")
                }
            );
        }
    }

    #[test]
    fn a_drive_root_parent_matches_an_explicit_root_cwd() {
        assert_eq!(path_key(parent_dir("C:\\tool.exe")), path_key("C:\\"));
        assert_eq!(parent_dir("C:\\Tools\\a.exe"), "C:\\Tools");
    }

    #[test]
    fn different_arguments_or_cwd_remain_distinct() {
        let target = "C:\\Tools\\editor.exe";
        let entries = vec![
            shortcut(
                "Editor",
                AppSource::UserStartMenu,
                "C:\\s\\a.lnk",
                classic(target, "--new", "C:\\work"),
            ),
            shortcut(
                "Editor",
                AppSource::UserStartMenu,
                "C:\\s\\b.lnk",
                classic(target, "--new", "C:\\other"),
            ),
            shortcut(
                "Editor",
                AppSource::UserStartMenu,
                "C:\\s\\c.lnk",
                classic(target, "--NEW", "C:\\work"),
            ),
            // Same identity as the first after key normalization: merges.
            shortcut(
                "Editor (copy)",
                AppSource::CommonStartMenu,
                "C:\\s\\d.lnk",
                classic("c:/tools/./EDITOR.exe", "--new", "C:/Work"),
            ),
        ];
        let apps = dedup(entries);
        assert_eq!(apps.len(), 3);
        let merged = apps
            .iter()
            .find(|a| a.aliases == vec!["Editor (copy)".to_string()])
            .expect("normalized duplicate merges into the user shortcut");
        assert_eq!(merged.source, AppSource::UserStartMenu);
    }

    #[test]
    fn unresolved_shortcut_never_merges_with_resolved_entry() {
        let lnk = "C:\\s\\broken.lnk";
        let entries = vec![
            shortcut(
                "Broken",
                AppSource::UserStartMenu,
                lnk,
                AppId::UnresolvedShortcut {
                    path_key: path_key(lnk),
                },
            ),
            shortcut(
                "Broken",
                AppSource::UserStartMenu,
                lnk,
                classic(lnk, "", "C:\\s"),
            ),
        ];
        assert_eq!(dedup(entries).len(), 2);
    }

    #[test]
    fn packaged_aumid_shape() {
        for yes in [
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App",
            "36828agowa338.AlpineWSL_my43bytk1c4nr!alpinewsl",
        ] {
            assert!(is_packaged_aumid(yes), "{yes}");
        }
        for no in [
            "Brave",
            "com.squirrel.Discord.Discord",
            "E:\\Programs\\App\\App.exe",
            "::{645FF040-5081-101B-9F08-00AA002F954E}",
            "Name_8wekyb3d8bbwe",
            "Name_8WEKYB3D8BBWE!App",
            "Name_short!App",
            "_8wekyb3d8bbwe!App",
            "Name_8wekyb3d8bbwe!",
            "Name_8wekyb3d8bbwe!A!B",
            "C:\\x_8wekyb3d8bbwe!App",
        ] {
            assert!(!is_packaged_aumid(no), "{no}");
        }
    }

    #[test]
    fn shortcut_identity_rules() {
        let packaged: BTreeSet<String> = [aumid_key("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App")]
            .into_iter()
            .collect();
        let target = ShortcutTarget {
            target: "C:\\Apps\\Brave\\brave.exe".into(),
            arguments: "--profile=2".into(),
            working_dir: String::new(),
        };

        // A packaged AUMID known to AppsFolder wins over the resolved target.
        assert_eq!(
            shortcut_id(
                "C:\\s\\t.lnk",
                Some(&target),
                Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
                &packaged
            ),
            AppId::Packaged {
                aumid_key: "microsoft.windowsterminal_8wekyb3d8bbwe!app".into()
            }
        );
        // A classic app's own AUMID does not merge launch actions.
        let classic = shortcut_id("C:\\s\\b.lnk", Some(&target), Some("Brave"), &packaged);
        assert_eq!(
            classic,
            AppId::Classic {
                target_key: "c:\\apps\\brave\\brave.exe".into(),
                arguments: "--profile=2".into(),
                cwd_key: "c:\\apps\\brave".into(),
            }
        );
        // An explicit working directory equal to the target folder is the same action.
        let explicit = ShortcutTarget {
            working_dir: "C:/Apps/Brave".into(),
            ..target.clone()
        };
        assert_eq!(
            shortcut_id("C:\\s\\c.lnk", Some(&explicit), None, &packaged),
            classic
        );
        // No resolvable filesystem target: keyed by the .lnk itself.
        for unresolved in [None, Some(&ShortcutTarget::default())] {
            assert_eq!(
                shortcut_id("C:\\S\\Store.lnk", unresolved, None, &packaged),
                AppId::UnresolvedShortcut {
                    path_key: "c:\\s\\store.lnk".into()
                }
            );
        }
    }

    #[test]
    fn builtin_commands_have_stable_ids() {
        let commands = builtin_commands(true);
        assert_eq!(commands.len(), Command::ALL.len() + SystemAction::ALL.len());
        assert_eq!(builtin_commands(false).len(), commands.len() - 1);
        assert_eq!(
            serde_json::to_string(&commands[0].id).unwrap(),
            r#"{"kind":"command","name":"clipboard-history"}"#
        );
        assert_eq!(commands[0].source, AppSource::Builtin);
        let about = commands
            .iter()
            .find(|c| c.id == Command::About.id())
            .unwrap();
        assert_eq!(about.display_name, "About smolcast");
        assert_eq!(about.aliases, vec!["Credits"]);
        // Usage keys never collide between commands and system actions.
        let ids: BTreeSet<&AppId> = commands.iter().map(|c| &c.id).collect();
        assert_eq!(ids.len(), commands.len());
        let lock = commands
            .iter()
            .find(|c| c.launch == LaunchSpec::System(SystemAction::LockScreen))
            .unwrap();
        assert_eq!(
            serde_json::to_string(&lock.id).unwrap(),
            r#"{"kind":"command","name":"system-lock-screen"}"#
        );
    }

    #[test]
    fn only_session_ending_and_deleting_actions_confirm() {
        let confirming: Vec<SystemAction> = SystemAction::ALL
            .into_iter()
            .filter(|a| a.confirms())
            .collect();
        assert_eq!(
            confirming,
            [
                SystemAction::Restart,
                SystemAction::ShutDown,
                SystemAction::SignOut,
                SystemAction::EmptyRecycleBin
            ]
        );
    }

    #[test]
    fn quicklink_entries_use_the_id_as_an_alias() {
        let mut links = BTreeMap::new();
        links.insert(
            "gh".to_string(),
            QuicklinkConfig {
                name: "Search GitHub".into(),
                url: "https://github.com/search?q={query}".into(),
            },
        );
        let entries = quicklink_entries(&links);
        assert_eq!(entries[0].id, AppId::Quicklink { id: "gh".into() });
        assert_eq!(entries[0].aliases, vec!["gh"]);
        assert_eq!(
            entries[0].launch,
            LaunchSpec::Quicklink {
                target: "https://github.com/search?q={query}".into()
            }
        );
    }

    #[test]
    fn script_entries_carry_the_exact_process_model() {
        let mut scripts = BTreeMap::new();
        scripts.insert(
            "open-project".to_string(),
            ScriptConfig {
                name: "Open project".into(),
                interpreter: "C:\\Program Files\\PowerShell\\7\\pwsh.exe".into(),
                args: vec![
                    "-NoProfile".into(),
                    "-File".into(),
                    "C:\\Tools\\my script \"x\" ✓.ps1".into(),
                ],
                cwd: "C:\\Users\\me\\Projects".into(),
                hotkey: Some("control+alt+KeyP".into()),
                path: None,
            },
        );
        let entries = script_entries(&scripts);
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.icon_key, "C:\\Program Files\\PowerShell\\7\\pwsh.exe");
        assert_eq!(
            entry.id,
            AppId::Script {
                id: "open-project".into()
            }
        );
        assert_eq!(entry.aliases, vec!["open-project"]);
        assert_eq!(entry.source, AppSource::Config);
        // Arguments stay one vector element each: no joining, quoting, or splitting.
        assert_eq!(
            entry.launch,
            LaunchSpec::Script(ScriptSpec {
                interpreter: "C:\\Program Files\\PowerShell\\7\\pwsh.exe".into(),
                args: vec![
                    "-NoProfile".into(),
                    "-File".into(),
                    "C:\\Tools\\my script \"x\" ✓.ps1".into(),
                ],
                cwd: "C:\\Users\\me\\Projects".into(),
                file: None,
            })
        );
        assert_eq!(
            serde_json::to_string(&entry.id).unwrap(),
            r#"{"kind":"script","id":"open-project"}"#
        );
    }

    #[test]
    fn usage_id_serializes_with_kind_tag() {
        let id = AppId::Packaged {
            aumid_key: "a!b".into(),
        };
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            r#"{"kind":"packaged","aumid_key":"a!b"}"#
        );
    }
}
