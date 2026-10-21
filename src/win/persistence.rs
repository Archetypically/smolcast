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

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::{GUID, PCWSTR};

use crate::app::model::{AppEvent, Wake};
use crate::app::notice::{Notice, Severity, op};
use crate::core::favorites::Favorites;
use crate::core::history::{Payload, decode_record};
use crate::core::usage::Usage;

/// Temp files older than this are leftovers from a crashed write (§10.5).
const STALE_TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Exact product paths (§10.1).
#[derive(Clone, Debug)]
pub struct StatePaths {
    /// `%APPDATA%\smolcast\config.toml`
    pub config: PathBuf,
    /// `%APPDATA%\smolcast\favorites.json`: user-curated, so it roams with
    /// the config rather than living with the usage cache.
    pub favorites: PathBuf,
    /// `%LOCALAPPDATA%\smolcast\usage.json`
    pub usage: PathBuf,
    /// `%LOCALAPPDATA%\smolcast\clipboard\`: one DPAPI-encrypted file per
    /// persisted history entry (§7.2).
    pub clipboard_dir: PathBuf,
}

impl StatePaths {
    pub fn resolve() -> Option<Self> {
        let roaming = known_folder(&FOLDERID_RoamingAppData)?.join("smolcast");
        let local = known_folder(&FOLDERID_LocalAppData)?.join("smolcast");
        Some(Self {
            config: roaming.join("config.toml"),
            favorites: roaming.join("favorites.json"),
            usage: local.join("usage.json"),
            clipboard_dir: local.join("clipboard"),
        })
    }
}

/// A known folder's current path (`%APPDATA%`, `%LOCALAPPDATA%`).
fn known_folder(id: &GUID) -> Option<PathBuf> {
    let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    let path = unsafe { raw.to_string() }.ok().map(PathBuf::from);
    unsafe { CoTaskMemFree(Some(raw.0 as *const _)) };
    path.filter(|p| p.is_absolute())
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

/// Load favorites. Missing means none. An unreadable file is renamed to
/// `favorites.corrupt-<unix_ms>.json`, never overwritten in place, since the
/// user curated it; one notice says where it went.
pub fn load_favorites(path: &Path) -> (Favorites, Option<Notice>) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (Favorites::default(), None),
        Err(e) => {
            return (
                Favorites::default(),
                Some(Notice::new(
                    Severity::Warning,
                    op::LAUNCHER,
                    format!("Could not read favorites ({e})."),
                )),
            );
        }
    };
    match Favorites::decode(&bytes) {
        Ok(favorites) => (favorites, None),
        Err(decode_error) => {
            let aside = path.with_file_name(format!("favorites.corrupt-{}.json", unix_ms_now()));
            let message = match fs::rename(path, &aside) {
                Ok(()) => format!(
                    "Favorites were unreadable ({decode_error}); the file was kept as {}.",
                    aside.display()
                ),
                Err(e) => format!(
                    "Favorites were unreadable ({decode_error}) and could not be moved aside \
                     ({e}); they will be replaced when you next change a favorite."
                ),
            };
            (
                Favorites::default(),
                Some(Notice::new(Severity::Warning, op::LAUNCHER, message)),
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

/// Persisted clipboard-history operations (§7.2), executed in order.
pub enum ClipboardStoreCommand {
    /// Encrypt and write one entry record (from `encode_record`).
    Save {
        token: String,
        record: Vec<u8>,
    },
    Delete {
        token: String,
    },
    /// Delete every stored entry.
    Clear,
    /// Decrypt and decode every stored entry; the result arrives as
    /// `AppEvent::ClipboardHistoryLoaded`.
    Load {
        item_cap: usize,
    },
}

/// One entry read back from disk.
#[derive(Debug)]
pub struct LoadedEntry {
    pub token: String,
    pub payload: Payload,
    pub captured_at_ms: i64,
}

const CLIP_EXTENSION: &str = "clip";
/// DPAPI secondary entropy: binds blobs to this purpose.
const CLIP_ENTROPY: &[u8] = b"smolcast.clipboard.v1";

enum WriterCommand {
    Usage {
        generation: u64,
        bytes: Vec<u8>,
    },
    /// The whole favorites document; the newest queued one wins.
    Favorites(Vec<u8>),
    Clipboard(ClipboardStoreCommand),
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
    pub fn start(paths: &StatePaths, wake: Wake) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let paths = paths.clone();
        let join = thread::Builder::new()
            .name("smolcast-state".into())
            .spawn(move || writer_main(rx, paths, wake))?;
        Ok(Self {
            tx,
            join: Some(join),
        })
    }

    /// Queue a usage snapshot. `generation` must increase with every snapshot.
    pub fn save_usage(&self, generation: u64, bytes: Vec<u8>) {
        let _ = self.tx.send(WriterCommand::Usage { generation, bytes });
    }

    pub fn save_favorites(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(WriterCommand::Favorites(bytes));
    }

    pub fn clipboard(&self, command: ClipboardStoreCommand) {
        let _ = self.tx.send(WriterCommand::Clipboard(command));
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

fn writer_main(rx: Receiver<WriterCommand>, paths: StatePaths, wake: Wake) {
    let StatePaths {
        usage: usage_path,
        clipboard_dir,
        favorites: favorites_path,
        ..
    } = paths;
    remove_stale_temps(&usage_path);
    remove_stale_temps(&favorites_path);
    let mut written_generation = 0u64;
    while let Ok(first) = rx.recv() {
        let mut newest: Option<(u64, Vec<u8>)> = None;
        let mut favorites: Option<Vec<u8>> = None;
        let mut stop = false;
        for command in std::iter::once(first).chain(rx.try_iter()) {
            match command {
                WriterCommand::Usage { generation, bytes } => {
                    if newest.as_ref().is_none_or(|(g, _)| generation > *g) {
                        newest = Some((generation, bytes));
                    }
                }
                WriterCommand::Favorites(bytes) => favorites = Some(bytes),
                // Clipboard operations are not coalesced: each runs in order.
                WriterCommand::Clipboard(command) => {
                    clipboard_store(&clipboard_dir, command, &wake)
                }
                WriterCommand::Shutdown => stop = true,
            }
        }
        if let Some(bytes) = favorites
            && let Err(e) = atomic_write(&favorites_path, &bytes)
        {
            wake.send(AppEvent::Notice(Notice::new(
                Severity::Warning,
                op::LAUNCHER,
                format!("Could not save favorites: {e}"),
            )));
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

fn clipboard_store(dir: &Path, command: ClipboardStoreCommand, wake: &Wake) {
    let failed = |e: String| {
        wake.send(AppEvent::Notice(Notice::new(
            Severity::Warning,
            op::CLIPBOARD,
            format!("Clipboard history storage: {e}"),
        )));
    };
    match command {
        ClipboardStoreCommand::Save { token, record } => {
            let path = dir.join(format!("{token}.{CLIP_EXTENSION}"));
            match dpapi_protect(&record) {
                Ok(blob) => {
                    if let Err(e) = atomic_write(&path, &blob) {
                        failed(e.to_string());
                    }
                }
                Err(e) => failed(format!("encrypt: {e}")),
            }
        }
        ClipboardStoreCommand::Delete { token } => {
            let _ = fs::remove_file(dir.join(format!("{token}.{CLIP_EXTENSION}")));
        }
        ClipboardStoreCommand::Clear => {
            for path in clip_files(dir) {
                let _ = fs::remove_file(path);
            }
        }
        ClipboardStoreCommand::Load { item_cap } => {
            let entries = load_clipboard(dir, item_cap);
            wake.send(AppEvent::ClipboardHistoryLoaded(entries));
        }
    }
}

/// Stored entry files, plus removal of this directory's stale temp files.
fn clip_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.extension().is_some_and(|e| e == CLIP_EXTENSION) {
            files.push(path);
        } else if let Some(dest) = name.find(".clip.").map(|i| &name[..i + 5])
            && is_temp_for(&name, dest)
        {
            remove_stale_temps(&dir.join(dest));
        }
    }
    files
}

/// Read every stored entry, oldest first. Files that cannot be decrypted or
/// decoded (for example after a Windows password reset invalidated the DPAPI
/// key, or a record over the current item cap) are deleted.
fn load_clipboard(dir: &Path, item_cap: usize) -> Vec<LoadedEntry> {
    let mut loaded = Vec::new();
    for path in clip_files(dir) {
        let Some(token) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        let decoded = fs::read(&path)
            .ok()
            .and_then(|blob| dpapi_unprotect(&blob).ok())
            .and_then(|plain| decode_record(&plain, item_cap).ok());
        match decoded {
            Some((payload, captured_at_ms)) => loaded.push(LoadedEntry {
                token,
                payload,
                captured_at_ms,
            }),
            None => {
                let _ = fs::remove_file(&path);
            }
        }
    }
    loaded.sort_by(|a, b| {
        a.captured_at_ms
            .cmp(&b.captured_at_ms)
            .then_with(|| a.token.cmp(&b.token))
    });
    loaded
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    }
}

/// Copy a DPAPI output blob, zero it, and free it.
fn take_blob(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe {
        std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
        LocalFree(Some(HLOCAL(out.pbData.cast())));
    }
    bytes
}

/// Encrypt for the current Windows user (DPAPI, user scope).
fn dpapi_protect(plain: &[u8]) -> io::Result<Vec<u8>> {
    let (input, entropy) = (blob(plain), blob(CLIP_ENTROPY));
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    }
    .map_err(io::Error::from)?;
    Ok(take_blob(out))
}

fn dpapi_unprotect(cipher: &[u8]) -> io::Result<Vec<u8>> {
    let (input, entropy) = (blob(cipher), blob(CLIP_ENTROPY));
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    }
    .map_err(io::Error::from)?;
    Ok(take_blob(out))
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
