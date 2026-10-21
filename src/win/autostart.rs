//! Opt-in login registration (plan §12.7): the per-user Run key
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `smolcast`.
//!
//! `start_at_login` in the config is the source of truth. Startup (with a valid
//! config) and every accepted reload reconcile the key to it: enabled writes the
//! quoted path of the running executable when the value is missing or differs,
//! so a moved exe re-registers itself; disabled deletes the value. Both are
//! idempotent, and quitting never touches the key. The Task Manager
//! enable/disable state (`StartupApproved\Run`) is the user's and is left alone.

use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, ERROR_UNSUPPORTED_TYPE, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW,
};
use windows::core::{PCWSTR, w};

use crate::win::{from_wide_nul, wide};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("smolcast");

/// A failed registry or path operation. Content-free: names the step only.
#[derive(Debug)]
pub struct AutostartError {
    pub operation: &'static str,
    pub code: Option<u32>,
}

impl std::fmt::Display for AutostartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "could not {} the login entry", self.operation)
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
    let key = open_run_key()?;
    let current = read_value(&key)?;
    if !enabled {
        if current.is_none() {
            return Ok(false);
        }
        check(unsafe { RegDeleteValueW(key.0, VALUE_NAME) }, "remove")?;
        return Ok(true);
    }
    let exe = std::env::current_exe().map_err(|e| AutostartError {
        operation: "locate the executable for",
        code: e.raw_os_error().map(|c| c as u32),
    })?;
    let command = format!("\"{}\"", exe.display());
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
        })
    }
}
