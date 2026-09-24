//! Persistence (plan §10.1, §10.4–§10.5): exact state paths, atomic replace,
//! usage quarantine, and the single state-writer thread.
//!
//! Every write goes to a same-directory `<name>.<pid>.<counter>.tmp`, is
//! flushed with `sync_all`, and replaces the destination with
//! `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`. A failure leaves the
//! previous destination untouched; nothing is ever truncated in place.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::core::PCWSTR;

use crate::app::model::{AppEvent, Wake};
use crate::app::notice::{Notice, Severity, op};
use crate::core::usage::Usage;

/// Temp files older than this are leftovers from a crashed write (§10.5).
const STALE_TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Exact product paths (§10.1).
#[derive(Clone, Debug)]
pub struct StatePaths {
    /// `%APPDATA%\smolcast\config.toml`
    pub config: PathBuf,
    /// `%LOCALAPPDATA%\smolcast\usage.json`
    pub usage: PathBuf,
}

impl StatePaths {
    pub fn resolve() -> Option<Self> {
        let base = directories::BaseDirs::new()?;
        Some(Self {
            config: base.config_dir().join("smolcast").join("config.toml"),
            usage: base.data_local_dir().join("smolcast").join("usage.json"),
        })
    }
}

/// A persistence failure. Carries the operation and path, never file content.
#[derive(Debug)]
pub struct PersistError {
    pub operation: &'static str,
    pub path: PathBuf,
    pub source: io::Error,
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.operation,
            self.path.display(),
            self.source
        )
    }
}

impl std::error::Error for PersistError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

fn err(operation: &'static str, path: &Path) -> impl FnOnce(io::Error) -> PersistError + use<> {
    let path = path.to_owned();
    move |source| PersistError {
        operation,
        path,
        source,
    }
}

/// Atomically replace `dest` with `bytes` (§10.5 steps 1–5).
pub fn atomic_write(dest: &Path, bytes: &[u8]) -> Result<(), PersistError> {
    let dir = dest
        .parent()
        .ok_or_else(|| err("resolve directory of", dest)(io::ErrorKind::InvalidInput.into()))?;
    fs::create_dir_all(dir).map_err(err("create directory", dir))?;

    let temp = dir.join(temp_name(dest, std::process::id(), next_counter()));
    let result = write_synced(&temp, bytes)
        .and_then(|()| move_replace(&temp, dest).map_err(err("replace", dest)));
    if result.is_err() {
        // Best effort: the temp file is ours and never valid state.
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_synced(temp: &Path, bytes: &[u8]) -> Result<(), PersistError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .map_err(err("create", temp))?;
    file.write_all(bytes).map_err(err("write", temp))?;
    file.sync_all().map_err(err("flush", temp))
}

fn move_replace(from: &Path, to: &Path) -> io::Result<()> {
    let from = wide(from);
    let to = wide(to);
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(io::Error::from)
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn next_counter() -> u64 {
    TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// `<file name>.<pid>.<counter>.tmp`
fn temp_name(dest: &Path, pid: u32, counter: u64) -> String {
    let name = dest.file_name().unwrap_or_default().to_string_lossy();
    format!("{name}.{pid}.{counter}.tmp")
}

/// True only for names produced by [`temp_name`] for `dest_name`.
fn is_temp_for(file_name: &str, dest_name: &str) -> bool {
    let Some(rest) = file_name
        .strip_prefix(dest_name)
        .and_then(|r| r.strip_prefix('.'))
        .and_then(|r| r.strip_suffix(".tmp"))
    else {
        return false;
    };
    let mut parts = rest.split('.');
    let numeric =
        |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    numeric(parts.next()) && numeric(parts.next()) && parts.next().is_none()
}

/// Delete this destination's smolcast temp files older than 24 hours (§10.5).
/// Returns how many were removed; failures are ignored because a leftover temp
/// file is never read as state.
pub fn remove_stale_temps(dest: &Path) -> usize {
    let (Some(dir), Some(dest_name)) = (dest.parent(), dest.file_name()) else {
        return 0;
    };
    let dest_name = dest_name.to_string_lossy();
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    entries
        .flatten()
        .filter(|entry| is_temp_for(&entry.file_name().to_string_lossy(), &dest_name))
        .filter(|entry| {
            entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > STALE_TEMP_AGE)
        })
        .filter(|entry| fs::remove_file(entry.path()).is_ok())
        .count()
}

/// Load usage state (§10.4). Missing means empty. A malformed or unsupported
/// file is renamed to `usage.corrupt-<unix_ms>.json` and replaced by empty
/// state, with one notice.
pub fn load_usage(path: &Path) -> (Usage, Option<Notice>) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (Usage::default(), None),
        Err(e) => {
            return (
                Usage::default(),
                Some(Notice::new(
                    Severity::Warning,
                    op::USAGE,
                    format!("Could not read launch history ({e}); starting empty."),
                )),
            );
        }
    };
    match Usage::decode(&bytes) {
        Ok(usage) => (usage, None),
        Err(decode_error) => {
            let quarantine = path.with_file_name(format!("usage.corrupt-{}.json", unix_ms_now()));
            let message = match fs::rename(path, &quarantine) {
                Ok(()) => format!(
                    "Launch history was unreadable ({decode_error}) and was reset; the old file \
                     was kept as {}.",
                    quarantine.display()
                ),
                Err(e) => format!(
                    "Launch history was unreadable ({decode_error}) and was reset; moving it \
                     aside failed ({e}), so it will be replaced on the next launch."
                ),
            };
            (
                Usage::default(),
                Some(Notice::new(Severity::Warning, op::USAGE, message)),
            )
        }
    }
}

/// Current UTC time in Unix milliseconds; a pre-1970 clock reads as 0.
pub fn unix_ms_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

enum WriterCommand {
    Usage { generation: u64, bytes: Vec<u8> },
    Shutdown,
}

/// The one persistence worker (§10.5). It serializes writes and coalesces
/// queued usage snapshots to the newest generation, so an older snapshot can
/// never replace a newer one.
pub struct StateWriter {
    tx: Sender<WriterCommand>,
    join: Option<JoinHandle<()>>,
}

impl StateWriter {
    pub fn start(usage_path: PathBuf, wake: Wake) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name("smolcast-state".into())
            .spawn(move || writer_main(rx, usage_path, wake))?;
        Ok(Self {
            tx,
            join: Some(join),
        })
    }

    /// Queue a usage snapshot. `generation` must increase with every snapshot.
    pub fn save_usage(&self, generation: u64, bytes: Vec<u8>) {
        let _ = self.tx.send(WriterCommand::Usage { generation, bytes });
    }

    /// Flush queued snapshots, stop, and join (§1.5 step 4).
    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = self.tx.send(WriterCommand::Shutdown);
            let _ = join.join();
        }
    }
}

impl Drop for StateWriter {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn writer_main(rx: Receiver<WriterCommand>, usage_path: PathBuf, wake: Wake) {
    remove_stale_temps(&usage_path);
    let mut written_generation = 0u64;
    while let Ok(first) = rx.recv() {
        let mut newest: Option<(u64, Vec<u8>)> = None;
        let mut stop = false;
        for command in std::iter::once(first).chain(rx.try_iter()) {
            match command {
                WriterCommand::Usage { generation, bytes } => {
                    if newest.as_ref().is_none_or(|(g, _)| generation > *g) {
                        newest = Some((generation, bytes));
                    }
                }
                WriterCommand::Shutdown => stop = true,
            }
        }
        if let Some((generation, bytes)) = newest
            && generation > written_generation
        {
            match atomic_write(&usage_path, &bytes) {
                Ok(()) => written_generation = generation,
                Err(e) => wake.send(AppEvent::Notice(Notice::new(
                    Severity::Warning,
                    op::USAGE,
                    format!("Could not save launch history: {e}"),
                ))),
            }
        }
        if stop {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_names_round_trip_and_reject_lookalikes() {
        let dest = Path::new("C:\\x\\usage.json");
        let name = temp_name(dest, 4242, 7);
        assert_eq!(name, "usage.json.4242.7.tmp");
        assert!(is_temp_for(&name, "usage.json"));
        for other in [
            "usage.json",
            "usage.json.tmp",
            "usage.json.1.tmp",
            "usage.json.1.2.3.tmp",
            "usage.json.a.2.tmp",
            "usage.json..2.tmp",
            "config.toml.1.2.tmp",
            "usage.json.1.2.tmp.bak",
            "usage.corrupt-1.json",
        ] {
            assert!(!is_temp_for(other, "usage.json"), "{other}");
        }
    }
}
