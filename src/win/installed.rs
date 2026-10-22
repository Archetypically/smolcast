//! Reads the Uninstall registry keys for the launcher's installed-programs
//! source (plan §6.1, decision 027). Filtering lives in
//! `core::installed_programs`.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLongPathNameW};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, REG_ROUTINE_FLAGS,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};
use windows::core::{PCWSTR, PWSTR, w};

use crate::core::installed_programs::UninstallEntry;
use crate::win::{from_wide_nul, wide, wide_path};

/// `GetDriveTypeW` for a local hard disk (`WindowsProgramming`'s
/// `DRIVE_FIXED`, a feature smolcast does not otherwise need).
const DRIVE_FIXED: u32 = 3;
/// Registry key names are at most 255 characters.
const KEY_NAME_BUF: usize = 256;
/// Reads of a value that grew between the size query and the read.
const READ_ATTEMPTS: usize = 3;

/// Every visible, top-level subkey of the machine (64- and 32-bit views) and
/// current-user Uninstall keys. A missing or unreadable key contributes
/// nothing.
pub fn uninstall_entries() -> Vec<UninstallEntry> {
    let mut out = Vec::new();
    for (root, path) in [
        (
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        ),
        (
            HKEY_LOCAL_MACHINE,
            w!("SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        ),
        (
            HKEY_CURRENT_USER,
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        ),
    ] {
        let Some(key) = Key::open(root, path) else {
            continue;
        };
        for index in 0.. {
            let mut name = [0u16; KEY_NAME_BUF];
            let mut len = name.len() as u32;
            let status = unsafe {
                RegEnumKeyExW(
                    key.0,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            // ERROR_NO_MORE_ITEMS ends the list; any other failure (the key
            // deleted mid-scan) would repeat for every index.
            if status != ERROR_SUCCESS {
                break;
            }
            name[len as usize] = 0;
            if let Some(entry) =
                Key::open(key.0, PCWSTR(name.as_ptr())).and_then(|sub| read_entry(&sub))
            {
                out.push(entry);
            }
        }
    }
    out
}

/// `None` for rows Apps & features hides or nests (checked before the
/// strings are read).
fn read_entry(key: &Key) -> Option<UninstallEntry> {
    // An empty ParentKeyName or ReleaseType marks nothing.
    let set = |value| read_string(key, value).is_some_and(|s| !s.trim().is_empty());
    if read_dword(key, w!("SystemComponent")) == Some(1)
        || set(w!("ParentKeyName"))
        || set(w!("ReleaseType"))
    {
        return None;
    }
    let text = |value| read_string(key, value).unwrap_or_default();
    Some(UninstallEntry {
        display_name: text(w!("DisplayName")),
        display_icon: text(w!("DisplayIcon")),
        display_version: text(w!("DisplayVersion")),
    })
}

/// A string value; `REG_EXPAND_SZ` arrives with variables expanded, so the
/// first size can fall short and the read is retried at the size it reports.
fn read_string(key: &Key, value: PCWSTR) -> Option<String> {
    let mut bytes = 0u32;
    if get(key, value, RRF_RT_REG_SZ, None, &mut bytes) != ERROR_SUCCESS {
        return None;
    }
    for _ in 0..READ_ATTEMPTS {
        let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
        match get(
            key,
            value,
            RRF_RT_REG_SZ,
            Some(buf.as_mut_ptr().cast()),
            &mut bytes,
        ) {
            ERROR_SUCCESS => return Some(from_wide_nul(&buf)),
            ERROR_MORE_DATA => continue,
            _ => return None,
        }
    }
    None
}

fn read_dword(key: &Key, value: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut bytes = size_of::<u32>() as u32;
    let status = get(
        key,
        value,
        RRF_RT_REG_DWORD,
        Some((&mut data as *mut u32).cast()),
        &mut bytes,
    );
    (status == ERROR_SUCCESS).then_some(data)
}

/// `RegGetValueW` for a named value of `key`; `None` data queries the size
/// only.
fn get(
    key: &Key,
    value: PCWSTR,
    flags: REG_ROUTINE_FLAGS,
    data: Option<*mut core::ffi::c_void>,
    bytes: &mut u32,
) -> WIN32_ERROR {
    unsafe { RegGetValueW(key.0, PCWSTR::null(), value, flags, None, data, Some(bytes)) }
}

/// The long form of an existing file's path (`C:\PROGRA~1\…` becomes
/// `C:\Program Files\…`), so it compares with shortcut targets; `None` when
/// it is missing or a folder. Only fixed drives are probed, so a sleeping
/// share, mapped drive or removable disk cannot stall the scan.
pub fn existing_long_path(path: &Path) -> Option<PathBuf> {
    let path_str = path.as_os_str().to_string_lossy();
    let root = path_str.get(..3).filter(|r| r.ends_with(":\\"))?;
    if unsafe { GetDriveTypeW(PCWSTR(wide(root).as_ptr())) } != DRIVE_FIXED {
        return None;
    }
    let short = wide_path(path);
    let mut buf = vec![0u16; 1024];
    loop {
        let len = unsafe { GetLongPathNameW(PCWSTR(short.as_ptr()), Some(&mut buf)) } as usize;
        if len == 0 {
            return None;
        }
        if len < buf.len() {
            let long = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
            return long.is_file().then_some(long);
        }
        buf.resize(len, 0);
    }
}

/// An open registry key; closed on drop.
struct Key(HKEY);

impl Key {
    fn open(root: HKEY, path: PCWSTR) -> Option<Self> {
        let mut key = HKEY::default();
        let status =
            unsafe { RegOpenKeyExW(root, path, None, KEY_READ | KEY_WOW64_64KEY, &mut key) };
        (status == ERROR_SUCCESS).then_some(Self(key))
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}
