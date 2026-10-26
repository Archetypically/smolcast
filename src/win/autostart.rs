//! Opt-in login registration (plan §12.7): the per-user Run key
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `smolcast`.
//!
//! `start_at_login` in the config is the source of truth. Startup (with a valid
//! config) and every accepted reload reconcile the key to it: enabled writes the
//! quoted path of the running executable plus `LOGIN_FLAG` when the value is
//! missing or differs, so a moved exe (or an entry from before the flag)
//! re-registers itself; disabled deletes the value. Both are idempotent, and
//! quitting never touches the key. The Task Manager enable/disable state
//! (`StartupApproved\Run`) is the user's and is left alone.
//!
//! The Microsoft Store build (plan §15) registers through its manifest's
//! startup task instead: a Run value would point into the versioned package
//! folder and start the exe without package identity, so it removes a Run
//! value the portable exe left. Task Manager's switch is the task's
//! `DisabledByUser` state, which only the user can undo.

use std::sync::atomic::{AtomicU8, Ordering};

use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, ERROR_UNSUPPORTED_TYPE, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::win::{from_wide_nul, package, wide};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("smolcast");
/// The `TaskId` of the package manifest's `windows.startupTask` extension.
const STARTUP_TASK_ID: &str = "smolcast";

/// The command-line flag the Run entry passes, so a start at sign-in stays in
/// the tray while a start by the user opens the palette (decision 037).
pub const LOGIN_FLAG: &str = "--login";

/// A failed registry or path operation. Content-free: names the step only.
#[derive(Debug)]
pub struct AutostartError {
    pub operation: &'static str,
    pub code: Option<u32>,
    /// Why, when Windows refused rather than failed.
    pub reason: Option<&'static str>,
}

impl std::fmt::Display for AutostartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "could not {} the login entry", self.operation)?;
        match self.reason {
            Some(reason) => write!(f, ": {reason}"),
            None => Ok(()),
        }
    }
}

/// Closes the key on drop.
struct RunKey(HKEY);

impl Drop for RunKey {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

/// Make the Run value match `enabled`. Returns whether the key changed.
pub fn reconcile(enabled: bool) -> Result<bool, AutostartError> {
    if package::is_packaged() {
        return reconcile_startup_task(enabled);
    }
    if !enabled {
        return remove_run_value();
    }
    let key = open_run_key()?;
    let current = read_value(&key)?;
    let exe = std::env::current_exe().map_err(|e| AutostartError {
        operation: "locate the executable for",
        code: e.raw_os_error().map(|c| c as u32),
        reason: None,
    })?;
    let command = format!("\"{}\" {LOGIN_FLAG}", exe.display());
    if current.as_deref() == Some(command.as_str()) {
        return Ok(false);
    }
    let data: Vec<u8> = wide(&command)
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    check(
        unsafe { RegSetValueExW(key.0, VALUE_NAME, None, REG_SZ, Some(&data)) },
        "write",
    )?;
    Ok(true)
}

/// Make the package's startup task match `enabled`, once per change of the
/// setting: a reload that leaves it alone skips the startup-task broker,
/// which these calls block the UI thread on. A Run value left by the
/// portable exe is removed, or both copies would start at sign-in.
fn reconcile_startup_task(enabled: bool) -> Result<bool, AutostartError> {
    let requested = if enabled { REQUESTED_ON } else { REQUESTED_OFF };
    let previous = LAST_REQUESTED.load(Ordering::Relaxed);
    if previous == requested {
        return Ok(false);
    }
    let result = remove_run_value().and_then(|removed| {
        sync_startup_task(enabled, previous == UNKNOWN).map(|changed| changed || removed)
    });
    // A refusal is settled too: it is reported once, not on every reload.
    if result.as_ref().err().is_none_or(|e| e.reason.is_some()) {
        LAST_REQUESTED.store(requested, Ordering::Relaxed);
    }
    result
}

/// The `start_at_login` value last reconciled in the package.
static LAST_REQUESTED: AtomicU8 = AtomicU8::new(UNKNOWN);
const UNKNOWN: u8 = 0;
const REQUESTED_OFF: u8 = 1;
const REQUESTED_ON: u8 = 2;

/// Delete the Run value if present. Returns whether it existed.
fn remove_run_value() -> Result<bool, AutostartError> {
    let key = open_run_key()?;
    if read_value(&key)?.is_none() {
        return Ok(false);
    }
    check(unsafe { RegDeleteValueW(key.0, VALUE_NAME) }, "remove")?;
    Ok(true)
}

/// Enabling a desktop app's task shows no prompt. A task the user or a
/// policy turned off stays off; that is reported only when the user turns
/// the setting on, never at `startup`, as Task Manager's switch is theirs.
fn sync_startup_task(enabled: bool, startup: bool) -> Result<bool, AutostartError> {
    let failed = |operation| {
        move |e: windows::core::Error| AutostartError {
            operation,
            code: Some(e.code().0 as u32),
            reason: None,
        }
    };
    let refused = |reason| AutostartError {
        operation: "enable",
        code: None,
        reason: Some(reason),
    };
    let task = StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK_ID))
        .and_then(|operation| operation.join())
        .map_err(failed("open"))?;
    let state = task.State().map_err(failed("read"))?;
    match (enabled, state) {
        (true, StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy) => Ok(false),
        (true, StartupTaskState::DisabledByUser | StartupTaskState::DisabledByPolicy)
            if startup =>
        {
            Ok(false)
        }
        (true, StartupTaskState::DisabledByUser) => Err(refused(
            "it is turned off in Task Manager under Startup apps",
        )),
        (true, StartupTaskState::DisabledByPolicy) => {
            Err(refused("it is turned off by your organization"))
        }
        (true, _) => {
            let state = task
                .RequestEnableAsync()
                .and_then(|operation| operation.join())
                .map_err(failed("enable"))?;
            match state {
                StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => Ok(true),
                _ => Err(refused("Windows declined")),
            }
        }
        (false, StartupTaskState::Enabled) => {
            task.Disable().map_err(failed("remove"))?;
            Ok(true)
        }
        // Off, or on by policy, which an app cannot change.
        (false, _) => Ok(false),
    }
}

fn open_run_key() -> Result<RunKey, AutostartError> {
    let mut key = HKEY::default();
    check(
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        },
        "open",
    )?;
    Ok(RunKey(key))
}

/// The current value, or `None` when absent.
fn read_value(key: &RunKey) -> Result<Option<String>, AutostartError> {
    let mut bytes = 0u32;
    let status = unsafe {
        RegGetValueW(
            key.0,
            PCWSTR::null(),
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    // Some other tool wrote a non-string value; treat it as differing.
    if status == ERROR_UNSUPPORTED_TYPE {
        return Ok(Some(String::new()));
    }
    check(status, "read")?;
    let mut buffer = vec![0u16; (bytes as usize).div_ceil(2)];
    check(
        unsafe {
            RegGetValueW(
                key.0,
                PCWSTR::null(),
                VALUE_NAME,
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
        },
        "read",
    )?;
    Ok(Some(from_wide_nul(&buffer)))
}

fn check(status: WIN32_ERROR, operation: &'static str) -> Result<(), AutostartError> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(AutostartError {
            operation,
            code: Some(status.0),
            reason: None,
        })
    }
}
