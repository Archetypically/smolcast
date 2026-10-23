//! Windows API layer. Every `unsafe` call lives under this tree; safe modules
//! receive owned values and return typed errors. Handles, registered hotkeys,
//! clipboard ownership, COM, window classes, timers, and hooks use RAII guards
//! where ownership applies.

pub mod appearance;
pub mod autostart;
pub mod clipboard;
pub mod emoji_render;
pub mod file_search;
pub mod hotkey;
pub mod http;
pub mod icon;
pub mod installed;
pub mod paste;
pub mod persistence;
pub mod placement;
pub mod process;
pub mod shell;
pub mod single_instance;
pub mod system;
pub mod tray;
pub mod update;

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::PWSTR;

/// UTF-16 with a trailing NUL, for `PCWSTR` arguments.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A path as UTF-16 with a trailing NUL. Keeps unpaired surrogates.
pub(crate) fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Text before the first NUL. Invalid UTF-16 becomes U+FFFD.
pub(crate) fn from_wide_nul(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// The message and code of a Windows error.
pub(crate) fn win_error_parts(e: &windows::core::Error) -> (String, u32) {
    (e.message(), e.code().0 as u32)
}

/// Take ownership of a COM-allocated string.
pub(crate) fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}
