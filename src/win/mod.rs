//! Windows API layer. Every `unsafe` call lives under this tree; safe modules
//! receive owned values and return typed errors. Handles, registered hotkeys,
//! clipboard ownership, COM, window classes, timers, and hooks use RAII guards
//! where ownership applies.

pub mod appearance;
pub mod autostart;
pub mod clipboard;
pub mod crash_log;
pub mod emoji_render;
pub mod file_dialog;
pub mod file_search;
pub mod fonts;
pub mod hotkey;
pub mod http;
pub mod icon;
pub mod installed;
pub mod package;
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
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{HKEY, REG_ROUTINE_FLAGS, RegGetValueW};
use windows::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::{GUID, PCWSTR, PWSTR};

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

/// A known folder's current path (`%APPDATA%`, `%LOCALAPPDATA%`).
pub(crate) fn known_folder(id: &GUID) -> Option<PathBuf> {
    let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    let path = PathBuf::from(take_pwstr(raw));
    path.is_absolute().then_some(path)
}

/// A Windows program by its full path in known folder `folder`. A bare name
/// would make `Command` look beside smolcast.exe first, where a downloaded
/// file of the same name could be waiting. Falls back to the bare name.
pub(crate) fn windows_program(folder: &GUID, name: &str) -> PathBuf {
    known_folder(folder).map_or_else(|| PathBuf::from(name), |dir| dir.join(name))
}

/// Join `join`, or with a `deadline` wait only until then: a thread still
/// busy is left to end with the process, so a hung call cannot hang Quit
/// (plan §1.5).
pub(crate) fn join_by(join: JoinHandle<()>, deadline: Option<Instant>) {
    let Some(deadline) = deadline else {
        let _ = join.join();
        return;
    };
    while !join.is_finished() {
        if Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = join.join();
}

/// `RegGetValueW` for one value under `root`; `Some` when it succeeded and
/// `data` (`bytes` long) was filled.
pub(crate) fn read_value(
    root: HKEY,
    subkey: PCWSTR,
    value: PCWSTR,
    flags: REG_ROUTINE_FLAGS,
    data: *mut core::ffi::c_void,
    bytes: &mut u32,
) -> Option<()> {
    let status = unsafe { RegGetValueW(root, subkey, value, flags, None, Some(data), Some(bytes)) };
    (status == ERROR_SUCCESS).then_some(())
}
