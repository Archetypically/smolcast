//! The local crash log (plan §14). A panic hook appends the panic to
//! `crash.log` before the release build aborts; the next start says so
//! once, and Report a Bug can show the last entry. Nothing is sent.
//!
//! Only panics are caught: a crash in native code (an access violation, a
//! stack overflow) ends the process without running the hook.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};
use windows::core::{PCWSTR, w};

use crate::core::crash_log::{self, Crash, LastCrash, MAX_LOG_BYTES};
use crate::win::{from_wide_nul, read_value};

/// The previous log, kept when `crash.log` outgrows `MAX_LOG_BYTES`.
const PREVIOUS: &str = "crash.old.log";
/// An empty file the hook leaves for the next start to find.
const UNSEEN: &str = "crash.unseen";

/// Log every panic to `log`, then run the default hook.
pub fn install(log: PathBuf, windows: String) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let location = info.location().map(ToString::to_string).unwrap_or_default();
        let unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let text = crash_log::entry(&Crash {
            unix_secs,
            version: env!("CARGO_PKG_VERSION"),
            windows: &windows,
            thread: thread.name().unwrap_or("unnamed"),
            location: &location,
            message: info.payload_as_str().unwrap_or("(no message)"),
        });
        append(&log, &text);
        previous(info);
    }));
}

/// Best effort: a hook that fails to write still lets the process end.
fn append(log: &Path, text: &str) {
    let Some(dir) = log.parent() else {
        return;
    };
    let _ = fs::create_dir_all(dir);
    if fs::metadata(log).is_ok_and(|m| m.len() + text.len() as u64 > MAX_LOG_BYTES) {
        let _ = fs::rename(log, dir.join(PREVIOUS));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log) {
        let _ = file.write_all(text.as_bytes());
    }
    let _ = fs::write(dir.join(UNSEEN), b"");
}

/// Whether a crash was logged since the last start; clears the flag.
pub fn take_unseen(log: &Path) -> bool {
    log.parent()
        .is_some_and(|dir| fs::remove_file(dir.join(UNSEEN)).is_ok())
}

/// The newest entry, from `crash.log` or, just after it was set aside,
/// the previous log.
pub fn last_crash(log: &Path) -> Option<LastCrash> {
    let read = |path: &Path| {
        fs::read(path)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    read(log)
        .and_then(|text| crash_log::last_entry(&text))
        .or_else(|| {
            let previous = log.parent()?.join(PREVIOUS);
            crash_log::last_entry(&read(&previous)?)
        })
}

/// The running Windows, as `Windows 11 24H2 (build 26100.2314)`.
pub fn windows_label() -> String {
    let build = read_string(w!("CurrentBuildNumber"))
        .and_then(|b| b.trim().parse().ok())
        .unwrap_or(0);
    let display = read_string(w!("DisplayVersion"));
    crash_log::windows_label(build, read_dword(w!("UBR")), display.as_deref())
}

const VERSION_KEY: PCWSTR = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");

fn read_string(value: PCWSTR) -> Option<String> {
    // Every value read here is a short version string.
    let mut buf = [0u16; 64];
    let mut bytes = size_of_val(&buf) as u32;
    read_value(
        HKEY_LOCAL_MACHINE,
        VERSION_KEY,
        value,
        RRF_RT_REG_SZ,
        buf.as_mut_ptr().cast(),
        &mut bytes,
    )
    .map(|()| from_wide_nul(&buf))
}

fn read_dword(value: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut bytes = size_of::<u32>() as u32;
    read_value(
        HKEY_LOCAL_MACHINE,
        VERSION_KEY,
        value,
        RRF_RT_REG_DWORD,
        (&mut data as *mut u32).cast(),
        &mut bytes,
    )
    .map(|()| data)
}
