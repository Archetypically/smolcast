//! Launcher index models: stable app identity, comparison keys, and
//! deterministic duplicate selection.
//!
//! Windows-free and host-testable. The Windows shell scanner (`win/shell.rs`)
//! resolves shortcuts and AppsFolder items into [`RawEntry`] values with
//! comparison keys already normalized by [`path_key`] / [`aumid_key`]; this
//! module owns identity and merging only.
//!
//! Contract (see `plans/IMPLEMENTATION_PLAN.md` §6.1–§6.2):
//! - A shortcut carrying an AppUserModelID and an AppsFolder item with the same
//!   normalized AUMID share `AppId::Packaged`.
//! - Resolved classic shortcuts merge only when target, raw arguments, and
//!   working directory all match.
//! - An unresolved shortcut is keyed by its own `.lnk` path and never merges
//!   with a resolved entry.
//! - Duplicates keep one launch action chosen by source priority (current-user
//!   Start Menu, common Start Menu, AppsFolder); discarded names become aliases.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
}

/// Where an entry was enumerated. Declaration order is selection priority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AppSource {
    UserStartMenu,
    CommonStartMenu,
    AppsFolder,
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

fn parent_dir(path: &str) -> &str {
    path.rfind(['\\', '/']).map_or("", |i| &path[..i])
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
