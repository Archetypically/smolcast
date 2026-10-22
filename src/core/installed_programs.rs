//! Installed programs from the Uninstall registry keys (plan §6.1, decision
//! 027): the filter that turns "Apps & features" rows into launcher entries.
//!
//! Windows-free and host-testable. `win/installed.rs` reads the keys into
//! [`UninstallEntry`] values; this module decides which rows name a
//! launchable app and drops those the Start Menu already covers.

use std::path::{Path, PathBuf};

use crate::core::app_index::{AppId, AppSource, LaunchSpec, RawEntry, path_key};

/// One Uninstall subkey, as read from the registry. The reader has already
/// dropped rows Apps & features hides (`SystemComponent` = 1) or nests under
/// another (`ParentKeyName`, `ReleaseType`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UninstallEntry {
    pub display_name: String,
    /// `DisplayIcon`: a path, optionally quoted, optionally `,<index>`.
    pub display_icon: String,
    pub display_version: String,
}

/// Executable file stems (lowercase substrings) that install, update or
/// remove software rather than run it.
const EXCLUDED_STEMS: &[&str] = &[
    "setup",
    "unins",
    "redist",
    "maintenance",
    "installer",
    "update",
    "dpinst",
];

/// Executable file names that host another product's icon or repair action.
const EXCLUDED_NAMES: &[&str] = &["officec2rclient.exe"];

/// Folders (lowercase path-key substrings) whose exes are not the app:
/// cached installers, Windows' own hosts (`msiexec`, `rundll32`), and
/// packaged apps, which AppsFolder already lists and which do not start
/// from their exe.
const EXCLUDED_FOLDERS: &[&str] = &["\\package cache\\", ":\\windows\\", "\\windowsapps\\"];

/// Folder names (lowercase) shared by many programs: a shortcut whose
/// target sits directly in one covers only that folder, never those under it.
const SHARED_PARENTS: &[&str] = &[
    "program files",
    "program files (x86)",
    "programdata",
    "users",
    "appdata",
    "local",
    "roaming",
    "programs",
    "games",
    "desktop",
    "documents",
    "downloads",
];

/// The executable a row should launch, or `None` when the row has no name or
/// its icon is not an app executable. The path is not probed; the caller
/// checks it exists.
pub fn program_executable(entry: &UninstallEntry) -> Option<PathBuf> {
    if display_name(entry).is_empty() {
        return None;
    }
    // String work on the key, not `Path`: host tests run off Windows.
    let exe = icon_path(&entry.display_icon);
    let key = path_key(exe);
    let absolute = key.starts_with("\\\\") || key.get(1..3) == Some(":\\");
    let file = &key[key.rfind('\\').map_or(0, |i| i + 1)..];
    let stem = file.strip_suffix(".exe")?;
    if !absolute
        || EXCLUDED_NAMES.contains(&file)
        || EXCLUDED_FOLDERS.iter().any(|folder| key.contains(folder))
        || EXCLUDED_STEMS.iter().any(|word| stem.contains(word))
    {
        return None;
    }
    Some(PathBuf::from(exe))
}

/// `DisplayIcon` without quotes or a trailing `,<index>`, which installers
/// write inside or outside the quotes.
fn icon_path(display_icon: &str) -> &str {
    let s = display_icon.trim().trim_matches('"');
    let s = match s.rsplit_once(',') {
        Some((path, index)) if index.trim().parse::<i32>().is_ok() => path.trim_end(),
        _ => s,
    };
    s.trim_matches('"')
}

/// The display name without a trailing, space-separated copy of
/// `DisplayVersion` ("PDFgear 2.1.14" becomes "PDFgear").
pub fn display_name(entry: &UninstallEntry) -> &str {
    let name = entry.display_name.trim();
    let version = entry.display_version.trim();
    name.strip_suffix(version)
        .filter(|rest| rest.ends_with(char::is_whitespace))
        .map_or(name, str::trim_end)
}

/// Launcher rows for installed programs, leaving out any whose exe sits in
/// (or under, see [`covers`]) the folder of a Start Menu shortcut's target:
/// the shortcut already offers that app, often naming a sibling launcher exe
/// or a versioned subfolder. `resolve` returns an existing exe's long path
/// (expanding `PROGRA~1`), or `None` to skip it.
pub fn installed_entries(
    programs: &[UninstallEntry],
    existing: &[RawEntry],
    resolve: impl Fn(&Path) -> Option<PathBuf>,
) -> Vec<RawEntry> {
    let covered: Vec<&str> = existing
        .iter()
        .filter_map(|entry| match &entry.id {
            AppId::Classic { target_key, .. } => Some(parent_key(target_key)),
            _ => None,
        })
        .filter(|folder| !folder.is_empty())
        .collect();
    programs
        .iter()
        .filter_map(|program| {
            let exe = resolve(&program_executable(program)?)?;
            let exe_str = exe.to_string_lossy().into_owned();
            let key = path_key(&exe_str);
            let folder = parent_key(&key).to_owned();
            if covered.iter().any(|c| covers(c, &folder)) {
                return None;
            }
            Some(RawEntry {
                id: AppId::Classic {
                    target_key: key,
                    arguments: String::new(),
                    cwd_key: folder,
                },
                display_name: display_name(program).to_owned(),
                source: AppSource::InstalledPrograms,
                launch: LaunchSpec::Executable { path: exe },
                icon_key: exe_str,
            })
        })
        .collect()
}

/// The folder part of a path key.
fn parent_key(key: &str) -> &str {
    key.rfind('\\').map_or("", |i| &key[..i])
}

/// Whether a shortcut target's folder covers a program folder: the same
/// folder, or a descendant when the shortcut folder belongs to one program,
/// not a drive root, a profile (`C:\Users\<name>`) or a [`SHARED_PARENTS`]
/// folder such as `C:\Program Files` or `AppData\Local`.
fn covers(shortcut_folder: &str, program_folder: &str) -> bool {
    if shortcut_folder == program_folder {
        return true;
    }
    let segments: Vec<&str> = shortcut_folder.split('\\').collect();
    let last = segments.last().copied().unwrap_or_default();
    let profile = segments.len() == 3 && segments[1] == "users";
    segments.len() >= 2
        && !last.ends_with(':')
        && !profile
        && !SHARED_PARENTS.contains(&last)
        && program_folder
            .strip_prefix(shortcut_folder)
            .is_some_and(|rest| rest.starts_with('\\'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(name: &str, icon: &str) -> UninstallEntry {
        UninstallEntry {
            display_name: name.into(),
            display_icon: icon.into(),
            ..Default::default()
        }
    }

    fn shortcut_to(target: &str) -> RawEntry {
        let key = path_key(target);
        RawEntry {
            id: AppId::Classic {
                cwd_key: parent_key(&key).to_owned(),
                target_key: key,
                arguments: String::new(),
            },
            display_name: "Shortcut".into(),
            source: AppSource::CommonStartMenu,
            launch: LaunchSpec::Shortcut {
                lnk_path: PathBuf::from("C:\\x.lnk"),
            },
            icon_key: String::new(),
        }
    }

    fn names(entries: &[RawEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.display_name.as_str()).collect()
    }

    #[test]
    fn icon_path_strips_quotes_and_index() {
        assert_eq!(icon_path("\"C:\\A\\a.exe\",0"), "C:\\A\\a.exe");
        assert_eq!(icon_path("C:\\A\\a.exe,-101"), "C:\\A\\a.exe");
        assert_eq!(icon_path("C:\\A,B\\a.exe"), "C:\\A,B\\a.exe");
        assert_eq!(icon_path("  C:\\A\\a.exe  "), "C:\\A\\a.exe");
        assert_eq!(icon_path("\"C:\\A\\a.exe,0\""), "C:\\A\\a.exe");
    }

    #[test]
    fn keeps_app_executables_only() {
        let ok = program(
            "Overwatch",
            "D:\\Blizzard\\Overwatch\\_retail_\\Overwatch.exe",
        );
        assert!(program_executable(&ok).is_some());
        for icon in [
            "C:\\Steam\\games\\abc.ico",
            "C:\\NVIDIA\\NVI2.dll",
            "relative\\app.exe",
            "",
            "C:\\ProgramData\\Package Cache\\{x}\\vcredist_x64.exe",
            "C:\\WINDOWS\\Installer\\{x}\\ARPPRODUCTICON.exe",
            "C:\\Program Files\\Tool\\Tool Setup.exe",
            "C:\\Program Files\\Tool\\tool-maintenance.exe",
            "C:\\Steam\\uninstall.exe",
            "C:\\App\\unins000.exe",
            "C:\\PROGRA~1\\DIFX\\873032~1\\DPINST~1.EXE",
            "C:\\Windows\\System32\\msiexec.exe",
            "C:\\Program Files\\WindowsApps\\App_1.0_x64__8wekyb3d8bbwe\\app.exe",
            "C:\\Program Files\\Common Files\\microsoft shared\\ClickToRun\\OfficeC2RClient.exe",
        ] {
            assert_eq!(program_executable(&program("X", icon)), None, "{icon}");
        }
    }

    #[test]
    fn unnamed_rows_are_skipped() {
        assert_eq!(program_executable(&program("  ", "C:\\X\\x.exe")), None);
    }

    #[test]
    fn display_name_drops_a_trailing_version() {
        let mut p = program("PDFgear 2.1.14", "");
        p.display_version = "2.1.14".into();
        assert_eq!(display_name(&p), "PDFgear");
        p.display_name = "Riot Client ".into();
        p.display_version = String::new();
        assert_eq!(display_name(&p), "Riot Client");
        // Only a space-separated version is dropped.
        p.display_name = "Final Fantasy7".into();
        p.display_version = "7".into();
        assert_eq!(display_name(&p), "Final Fantasy7");
        // A name that is only the version stays.
        p.display_name = "1.0".into();
        p.display_version = "1.0".into();
        assert_eq!(display_name(&p), "1.0");
    }

    #[test]
    fn programs_under_a_shortcut_folder_are_left_to_the_shortcut() {
        let programs = [
            program(
                "Overwatch",
                "D:\\Blizzard\\Overwatch\\_retail_\\Overwatch.exe",
            ),
            program(
                "Microsoft Edge",
                "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\155.0\\msedge.exe",
            ),
            program(
                "SteelSeries GG",
                "C:\\Program Files\\SteelSeries\\GG\\SteelSeriesGG.exe",
            ),
            program("Thing", "C:\\Program Files\\Thing\\thing.exe"),
            program(
                "Code",
                "C:\\Users\\Alice\\AppData\\Local\\Programs\\Code\\Code.exe",
            ),
            program("Game", "D:\\Game\\bin\\game.exe"),
        ];
        let shortcuts = [
            shortcut_to("C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe"),
            shortcut_to("C:\\Program Files\\SteelSeries\\GG\\SteelSeriesGGEZ.exe"),
            // Shallow folders cover only themselves.
            shortcut_to("D:\\tool.exe"),
            shortcut_to("C:\\Program Files\\loose.exe"),
            shortcut_to("C:\\Users\\Alice\\tool.exe"),
            shortcut_to("C:\\Users\\Alice\\AppData\\Local\\tool.exe"),
            // A program's own folder covers its subfolders.
            shortcut_to("D:\\Game\\Launcher.exe"),
        ];
        let entries = installed_entries(&programs, &shortcuts, |p| Some(p.to_owned()));
        assert_eq!(names(&entries), ["Overwatch", "Thing", "Code"]);
        assert_eq!(entries[0].source, AppSource::InstalledPrograms);
        assert_eq!(
            entries[0].launch,
            LaunchSpec::Executable {
                path: PathBuf::from("D:\\Blizzard\\Overwatch\\_retail_\\Overwatch.exe")
            }
        );
    }

    #[test]
    fn missing_executables_are_skipped() {
        let programs = [program("Gone", "C:\\Gone\\gone.exe")];
        assert!(installed_entries(&programs, &[], |_| None).is_empty());
    }

    #[test]
    fn the_resolved_long_path_is_compared_and_launched() {
        let programs = [program("Foo", "C:\\PROGRA~1\\Foo\\foo.exe")];
        let long = |_: &Path| Some(PathBuf::from("C:\\Program Files\\Foo\\foo.exe"));
        let shortcut = [shortcut_to("C:\\Program Files\\Foo\\foo.exe")];
        assert!(installed_entries(&programs, &shortcut, long).is_empty());
        let entries = installed_entries(&programs, &[], long);
        assert_eq!(
            entries[0].launch,
            LaunchSpec::Executable {
                path: PathBuf::from("C:\\Program Files\\Foo\\foo.exe")
            }
        );
    }
}
