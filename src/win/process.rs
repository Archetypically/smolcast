//! Detached script spawn (plan §10.3).
//!
//! The process is built directly from the configured interpreter, argument
//! vector, and working directory; nothing is interpolated. It runs without a
//! console window and with null stdio, and the child handle is dropped at
//! once: spawn success is success. Error messages name the interpreter or
//! directory but never the arguments.

use std::fs;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use windows::Win32::System::Threading::CREATE_NO_WINDOW;
use windows::Win32::UI::Shell::{FOLDERID_System, FOLDERID_Windows};

use crate::core::script_file::WINDOWS_POWERSHELL;
use crate::win::{known_folder, windows_program};

use crate::core::app_index::ScriptSpec;
use crate::win::shell::LaunchError;

pub fn spawn_script(spec: &ScriptSpec) -> Result<(), LaunchError> {
    let fail = |message: String, code: Option<u32>| LaunchError {
        operation: "run script",
        message,
        code,
    };
    // Runtime validation (§10.2): an absolute, existing working directory.
    let cwd = Path::new(&spec.cwd);
    if !cwd.is_absolute() {
        return Err(fail(
            format!("working directory is not an absolute path: {}", spec.cwd),
            None,
        ));
    }
    if !cwd.is_dir() {
        return Err(fail(
            format!("working directory does not exist: {}", spec.cwd),
            None,
        ));
    }
    if let Some(file) = &spec.file
        && !Path::new(file).is_file()
    {
        return Err(fail(format!("script file not found: {file}"), None));
    }
    // A tracked .ps1 names Windows PowerShell by its bare name; run the one
    // in System32, never one found first on PATH.
    let program = if spec.interpreter == WINDOWS_POWERSHELL {
        windows_program(&FOLDERID_System, r"WindowsPowerShell\v1.0\powershell.exe")
    } else if is_bare(&spec.interpreter) {
        find_program(&spec.interpreter)
            .ok_or_else(|| fail(format!("interpreter not found: {}", spec.interpreter), None))?
    } else {
        spec.interpreter.clone().into()
    };
    let spawned = Command::new(program)
        .args(&spec.args)
        .current_dir(cwd)
        .creation_flags(CREATE_NO_WINDOW.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        // Dropping the child releases the handle; smolcast does not wait.
        Ok(_child) => Ok(()),
        Err(e) => {
            let code = e.raw_os_error().map(|c| c as u32);
            let message = match e.kind() {
                io::ErrorKind::NotFound => {
                    format!("interpreter not found: {}", spec.interpreter)
                }
                io::ErrorKind::PermissionDenied => {
                    format!("permission denied starting {}", spec.interpreter)
                }
                _ => format!("could not start {}: {e}", spec.interpreter),
            };
            Err(fail(message, code))
        }
    }
}

/// A program name with no folder (`python.exe`, `node`).
fn is_bare(program: &str) -> bool {
    !program.is_empty() && !program.contains(['\\', '/', ':'])
}

/// A bare program name resolved as `Command` would, minus its first stop:
/// smolcast.exe's own folder, where a downloaded file of the same name
/// could be waiting. System32, then Windows, then each `PATH` folder; a
/// name without an extension tries each of `PATHEXT`'s.
fn find_program(name: &str) -> Option<PathBuf> {
    let extensions: Vec<String> = if Path::new(name).extension().is_some() {
        vec![String::new()]
    } else {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let system = [&FOLDERID_System, &FOLDERID_Windows]
        .into_iter()
        .filter_map(known_folder);
    let path = std::env::var_os("PATH").unwrap_or_default();
    system
        .chain(std::env::split_paths(&path).filter(|dir| dir.is_absolute()))
        .flat_map(|dir| {
            extensions
                .iter()
                .map(move |ext| dir.join(format!("{name}{ext}")))
        })
        // Not followed: an app execution alias (WindowsApps\python.exe)
        // is a reparse point that cannot be read as a file.
        .find(|candidate| fs::symlink_metadata(candidate).is_ok_and(|m| !m.is_dir()))
}
