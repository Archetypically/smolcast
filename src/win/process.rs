//! Detached script spawn (plan §10.3).
//!
//! The process is built directly from the configured interpreter, argument
//! vector, and working directory; nothing is interpolated. It runs without a
//! console window and with null stdio, and the child handle is dropped at
//! once: spawn success is success. Error messages name the interpreter or
//! directory but never the arguments.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use windows::Win32::System::Threading::CREATE_NO_WINDOW;

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
    let spawned = Command::new(&spec.interpreter)
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
