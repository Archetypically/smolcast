//! Single-instance named-mutex guard (plan §5.1).
//!
//! A second launch detects `ERROR_ALREADY_EXISTS` and exits code 0 before any
//! window, hotkey, tray, or worker is created. Ownership is retained until the
//! `SingleInstance` guard drops during ordered shutdown.

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, OpenProcessToken, ReleaseMutex,
};
use windows::core::{Error, PCWSTR, PWSTR};

use crate::win::{wide, win_error_parts};

#[derive(Debug)]
pub enum AcquireError {
    /// A live smolcast instance owns the mutex; the caller exits silently.
    AlreadyRunning,
    Windows {
        message: String,
        code: u32,
    },
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcquireError::AlreadyRunning => write!(f, "another smolcast instance is running"),
            AcquireError::Windows { message, code } => {
                write!(f, "{message} (0x{code:08X})")
            }
        }
    }
}

impl std::error::Error for AcquireError {}

/// Owns the process-wide instance mutex; released and closed on drop.
#[derive(Debug)]
pub struct SingleInstance {
    handle: HANDLE,
}

impl SingleInstance {
    /// Create `Local\smolcast-<sid>` and take ownership, or report that another
    /// instance already holds it.
    pub fn acquire() -> Result<Self, AcquireError> {
        let sid = current_user_sid()?;
        let name = wide(&format!("Local\\smolcast-{sid}"));

        // A valid handle is returned even when the mutex already exists, so the
        // error from the wrapper is a genuine NULL-handle failure (§5.1 step 3).
        let handle =
            unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.map_err(windows_err)?;
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            let _ = unsafe { ReleaseMutex(handle) };
            let _ = unsafe { CloseHandle(handle) };
            return Err(AcquireError::AlreadyRunning);
        }
        Ok(Self { handle })
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = unsafe { ReleaseMutex(self.handle) };
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

/// Format the current process token's user SID (§5.1 step 1).
fn current_user_sid() -> Result<String, AcquireError> {
    let mut token = HANDLE::default();
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(windows_err)?;
    }
    let result = (|| -> Result<String, AcquireError> {
        // First call only sizes the buffer; ERROR_INSUFFICIENT_BUFFER is expected.
        let mut needed = 0u32;
        let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut needed) };
        if needed == 0 {
            return Err(AcquireError::Windows {
                message: "GetTokenInformation reported a zero-size token".into(),
                code: 0,
            });
        }
        let mut buf = vec![0u8; needed as usize];
        unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                Some(buf.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
            .map_err(windows_err)?;
        }
        let token_user: TOKEN_USER = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast()) };

        let mut out = PWSTR::null();
        unsafe {
            ConvertSidToStringSidW(token_user.User.Sid, &mut out).map_err(windows_err)?;
        }
        if out.is_null() {
            return Err(AcquireError::Windows {
                message: "ConvertSidToStringSidW returned a null string".into(),
                code: 0,
            });
        }
        let sid = unsafe { out.to_string() }.map_err(|source| AcquireError::Windows {
            message: format!("SID string was not valid UTF-16: {source}"),
            code: 0,
        })?;
        // ConvertSidToStringSidW allocates with LocalAlloc; we must free it.
        unsafe { LocalFree(Some(HLOCAL(out.0.cast()))) };
        Ok(sid)
    })();
    let _ = unsafe { CloseHandle(token) };
    result
}

fn windows_err(error: Error) -> AcquireError {
    let (message, code) = win_error_parts(&error);
    AcquireError::Windows { message, code }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_twice_in_same_process_yields_already_running() {
        let first = SingleInstance::acquire().expect("first instance must acquire");
        // A second mutex with the same name in the same process is also
        // ERROR_ALREADY_EXISTS because bInitialOwner is held by the first handle.
        let second = SingleInstance::acquire();
        match second {
            Err(AcquireError::AlreadyRunning) => {}
            other => panic!("expected AlreadyRunning, got {other:?}"),
        }
        drop(first);
        // After release, acquisition succeeds again.
        let _third = SingleInstance::acquire().expect("re-acquire after release must work");
    }
}
