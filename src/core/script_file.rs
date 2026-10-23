//! Tracked script files (plan §10.3): the program, arguments and working
//! folder that run a file on disk, chosen from its extension, and the
//! arguments typed for one run.
//!
//! Windows-free and host-testable. Untracking removes the config entry and
//! never touches the file.

use crate::core::config::ScriptConfig;

/// Extensions the file picker offers, in its filter's order.
pub const EXTENSIONS: [&str; 9] = ["ps1", "cmd", "bat", "py", "pyw", "js", "mjs", "exe", "com"];

/// The program for `.ps1` files. The spawn runs the copy in System32 by
/// full path, so a `powershell.exe` earlier on PATH is never used.
pub const WINDOWS_POWERSHELL: &str = "powershell.exe";

/// The exact process a script runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resolved {
    pub interpreter: String,
    pub args: Vec<String>,
    pub cwd: String,
}

/// The program and the flags before the file for `path`'s extension, or
/// `None` when smolcast does not know how to run it. An executable runs
/// itself (an empty program). Windows PowerShell ships with every
/// Windows 11, so `.ps1` uses it; the default policy blocks every script,
/// so it runs under RemoteSigned instead.
fn runner(path: &str) -> Option<(&'static str, &'static [&'static str])> {
    let extension = extension(path)?;
    Some(match extension.as_str() {
        // RemoteSigned runs local scripts but still refuses downloaded ones
        // (Mark of the Web) until the user unblocks them. NonInteractive:
        // a hidden process with no input must fail rather than wait on a
        // prompt forever.
        "ps1" => (
            WINDOWS_POWERSHELL,
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "RemoteSigned",
                "-File",
            ],
        ),
        // A batch file is the program itself: the standard library then
        // starts cmd.exe with arguments escaped for it, and refuses any it
        // cannot escape. Passing the file to `cmd /c` instead would let cmd
        // re-parse the arguments, so `a&b` would start `b`.
        "cmd" | "bat" => ("", &[]),
        "py" => ("python.exe", &[]),
        "pyw" => ("pythonw.exe", &[]),
        "js" | "mjs" => ("node.exe", &[]),
        "exe" | "com" => ("", &[]),
        _ => return None,
    })
}

/// The dot before `file`'s extension, if it has one. A leading dot names the
/// file (`.gitignore`), not an extension.
fn extension_dot(file: &str) -> Option<usize> {
    file.rfind('.').filter(|&i| i > 0)
}

fn extension(path: &str) -> Option<String> {
    let file = file_name(path);
    let dot = extension_dot(file)?;
    Some(file[dot + 1..].to_ascii_lowercase())
}

/// A drive path (`C:\…`, `C:/…`) or a UNC path (`\\server\…`), checked
/// textually so it holds on any host. Device and verbatim paths (`\\.\`,
/// `\\?\`) are refused: they bypass normal path rules.
fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let unc = is_unc(path) && !path.starts_with("\\\\.\\") && !path.starts_with("\\\\?\\");
    drive || unc
}

/// A network path, `\\server\…`.
pub fn is_unc(path: &str) -> bool {
    path.starts_with("\\\\") && path.len() > 2
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// The file's folder; `C:` alone would be the drive's current folder, so a
/// root file gets `C:\`.
fn folder(path: &str) -> String {
    match path.rfind(['\\', '/']) {
        Some(i) if path[..i].ends_with(':') => format!("{}\\", &path[..i]),
        Some(i) => path[..i].to_owned(),
        None => String::new(),
    }
}

/// A name for a newly tracked file: its file name without the extension.
pub fn default_name(path: &str) -> String {
    let file = file_name(path);
    match extension_dot(file) {
        Some(i) => file[..i].to_owned(),
        None => file.to_owned(),
    }
}

/// Why a tracked `path` cannot run as configured: it is not absolute (a
/// relative path would depend on the folder, and one starting with `-`
/// would read as a flag), or smolcast does not know the extension and no
/// `interpreter` is set.
pub fn path_problem(path: &str, interpreter: &str) -> Option<String> {
    if !is_absolute(path) {
        return Some("path must be a full path, like C:\\Tools\\deploy.ps1".into());
    }
    if !interpreter.is_empty() || runner(path).is_some() {
        return None;
    }
    Some(match extension(path) {
        Some(ext) => format!("smolcast cannot run .{ext} files; set interpreter"),
        None => "the file has no extension; set interpreter".into(),
    })
}

/// The process for `script`, then `extra` arguments typed for this run.
///
/// A tracked file (`path`) runs with its extension's program and flags, or
/// with `interpreter` when set, then the path as one verbatim argument,
/// then `args`; the working folder defaults to the file's. Otherwise the
/// script is `interpreter`, `args` and `cwd` exactly as written.
pub fn resolve(script: &ScriptConfig, extra: &[String]) -> Resolved {
    let mut resolved = match &script.path {
        Some(path) => {
            let (program, flags) = match (script.interpreter.as_str(), runner(path)) {
                ("", Some((program, flags))) => (program, flags),
                (interpreter, _) => (interpreter, &[][..]),
            };
            let mut args: Vec<String> = flags.iter().map(|&f| f.to_owned()).collect();
            let interpreter = if program.is_empty() {
                path.clone()
            } else {
                args.push(path.clone());
                program.to_owned()
            };
            args.extend(script.args.iter().cloned());
            let cwd = if script.cwd.is_empty() {
                folder(path)
            } else {
                script.cwd.clone()
            };
            Resolved {
                interpreter,
                args,
                cwd,
            }
        }
        None => Resolved {
            interpreter: script.interpreter.clone(),
            args: script.args.clone(),
            cwd: script.cwd.clone(),
        },
    };
    resolved.args.extend(extra.iter().cloned());
    resolved
}

/// Split typed arguments: whitespace separates, double quotes group, and
/// `""` inside quotes is a literal quote. Backslashes are plain characters,
/// so a quoted folder may end in `\`. No shell sees the result.
pub fn split_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                current.push('"');
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

/// Arguments as one line [`split_args`] reads back: an argument that is
/// empty or holds whitespace or a quote is quoted, its quotes doubled.
pub fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if !arg.is_empty() && !arg.contains(|c: char| c.is_whitespace() || c == '"') {
                arg.clone()
            } else {
                format!("\"{}\"", arg.replace('"', "\"\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What runs a script, named for people.
pub fn language(script: &ScriptConfig) -> String {
    let interpreter = resolve(script, &[]).interpreter;
    let file = file_name(&interpreter).to_ascii_lowercase();
    let stem = file.strip_suffix(".exe").unwrap_or(&file);
    match stem {
        "powershell" => "Windows PowerShell".into(),
        "pwsh" => "PowerShell".into(),
        "cmd" => "Command Prompt".into(),
        _ if file.ends_with(".cmd") || file.ends_with(".bat") => "Command Prompt".into(),
        "python" | "pythonw" | "py" | "pyw" => "Python".into(),
        "node" => "Node.js".into(),
        "wsl" | "bash" => "Bash".into(),
        "" => "No program".into(),
        _ if file.ends_with(".exe") || file.ends_with(".com") => "Program".into(),
        _ => file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracked(path: &str, args: &[&str]) -> ScriptConfig {
        ScriptConfig {
            name: "x".into(),
            path: Some(path.into()),
            args: args.iter().map(|&a| a.into()).collect(),
            ..ScriptConfig::default()
        }
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|&a| a.into()).collect()
    }

    #[test]
    fn tracked_files_run_with_their_extensions_program_and_folder() {
        let r = resolve(
            &tracked("C:\\My Tools\\Open Project.PS1", &["-Env", "prod"]),
            &strings(&["now"]),
        );
        assert_eq!(r.interpreter, "powershell.exe");
        assert_eq!(
            r.args,
            strings(&[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "RemoteSigned",
                "-File",
                "C:\\My Tools\\Open Project.PS1",
                "-Env",
                "prod",
                "now"
            ])
        );
        assert_eq!(r.cwd, "C:\\My Tools");

        let r = resolve(&tracked("D:\\ü\\build.cmd", &[]), &strings(&["a&b"]));
        assert_eq!(r.interpreter, "D:\\ü\\build.cmd");
        assert_eq!(r.args, ["a&b"]);

        let r = resolve(&tracked("C:\\tool.exe", &["-v"]), &[]);
        assert_eq!(
            (r.interpreter.as_str(), r.cwd.as_str()),
            ("C:\\tool.exe", "C:\\")
        );
        assert_eq!(r.args, ["-v"]);

        let mut custom = tracked("C:\\x\\a.rb", &[]);
        assert!(path_problem("C:\\x\\a.rb", "").is_some());
        custom.interpreter = "ruby.exe".into();
        custom.cwd = "C:\\work".into();
        let r = resolve(&custom, &[]);
        assert_eq!(r.interpreter, "ruby.exe");
        assert_eq!(r.args, ["C:\\x\\a.rb"]);
        assert_eq!(r.cwd, "C:\\work");
        assert_eq!(path_problem("C:\\x\\a.rb", "ruby.exe"), None);
    }

    #[test]
    fn commands_run_verbatim() {
        let script = ScriptConfig {
            name: "x".into(),
            interpreter: "pwsh.exe".into(),
            args: strings(&["-c", "echo hi"]),
            cwd: "C:\\".into(),
            ..ScriptConfig::default()
        };
        let r = resolve(&script, &strings(&["more"]));
        assert_eq!(r.args, strings(&["-c", "echo hi", "more"]));
    }

    #[test]
    fn typed_arguments_split_like_a_windows_command_line_and_join_back() {
        assert_eq!(
            split_args(r#"  a "b c" "C:\x y\" "f""g" "" "#),
            strings(&["a", "b c", "C:\\x y\\", "f\"g", ""])
        );
        assert!(split_args("   ").is_empty());
        let args = strings(&["plain", "two words", "", "say \"hi\"", "C:\\a b\\"]);
        assert_eq!(split_args(&join_args(&args)), args);
    }

    #[test]
    fn shell_syntax_in_typed_arguments_stays_literal() {
        // Nothing is expanded or interpreted: each piece is one argv element.
        assert_eq!(
            split_args(r#"a&b | c>d %PATH% $(x) `y` ;z"#),
            strings(&["a&b", "|", "c>d", "%PATH%", "$(x)", "`y`", ";z"])
        );
        // An unbalanced quote runs to the end instead of being dropped.
        assert_eq!(
            split_args(r#"one "two three"#),
            strings(&["one", "two three"])
        );
        // Line breaks separate like spaces; no argument holds one.
        assert_eq!(split_args("a\r\nb\tc"), strings(&["a", "b", "c"]));
    }

    #[test]
    fn typed_arguments_follow_the_file_and_never_replace_it() {
        let extra = split_args(r#"-File C:\evil.ps1 "x y""#);
        let r = resolve(&tracked("C:\\Tools\\ok.ps1", &["-Env", "prod"]), &extra);
        let path_at = r.args.iter().position(|a| a == "C:\\Tools\\ok.ps1");
        assert_eq!(path_at, Some(5), "{:?}", r.args);
        assert_eq!(
            &r.args[6..],
            &strings(&["-Env", "prod", "-File", "C:\\evil.ps1", "x y"])[..]
        );
        // A path with spaces, quotes stripped by tracking, stays one element.
        let r = resolve(&tracked("C:\\My Tools\\ü run.py", &[]), &[]);
        assert_eq!(r.args, ["C:\\My Tools\\ü run.py"]);
    }

    #[test]
    fn only_absolute_paths_are_tracked() {
        for path in [
            "-c evil.py",
            "scripts\\a.ps1",
            "a.ps1",
            "C:a.ps1",
            "\\a.ps1",
            "\\\\?\\C:\\a.ps1",
            "\\\\.\\pipe\\a.ps1",
        ] {
            assert!(path_problem(path, "").is_some(), "{path}");
        }
        for path in ["C:\\a.ps1", "c:/x/a.py", "\\\\server\\share\\a.cmd"] {
            assert_eq!(path_problem(path, ""), None, "{path}");
        }
    }
}
