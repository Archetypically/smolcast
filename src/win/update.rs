//! Self-update from GitHub Releases (plan §13).
//!
//! One worker thread checks `releases/latest` once a day (when the
//! `check_for_updates` setting is on) or when asked, downloads a release the
//! user chose to install, checks its size and SHA-256, and swaps it in for
//! the running executable: a running exe cannot be overwritten, but it can
//! be renamed. The model then quits and the host starts the new exe, which
//! waits for this process to exit before taking the single-instance mutex.
//! Builds are not signed yet, so the checksum guards against a corrupt
//! download, not a hostile release.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
use windows::core::PCWSTR;

use crate::app::model::{AppEvent, Wake};
use crate::core::update::{
    self, LATEST_RELEASE_API, MAX_RELEASE_JSON_BYTES, Update, UpdateState, Version,
};
use crate::win::http::{self, HttpError};
use crate::win::persistence::{atomic_write, unix_ms_now};

/// The command-line flag a relaunched build gets: wait for this process id
/// to exit before starting.
pub const WAIT_PID_FLAG: &str = "--wait-pid";
/// How long a relaunched build waits for the old one to exit.
const WAIT_FOR_EXIT: Duration = Duration::from_secs(20);

/// What a check found.
#[derive(Clone, Debug)]
pub enum CheckOutcome {
    UpToDate,
    Available(Update),
    Failed(String),
}

enum Request {
    Check { manual: bool },
    Install(Update),
    Skip(Version),
    SetAutomatic(bool),
}

/// Owns the update thread; `shutdown` cancels a download and joins it.
pub struct UpdateWorker {
    tx: Option<Sender<Request>>,
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl UpdateWorker {
    /// `automatic` is the `check_for_updates` setting; debug builds never
    /// check on their own.
    pub fn start(state_path: PathBuf, automatic: bool, wake: Wake) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let join = thread::Builder::new()
            .name("smolcast-update".into())
            .spawn(move || update_main(rx, state_path, automatic, worker_cancel, wake))?;
        Ok(Self {
            tx: Some(tx),
            cancel,
            join: Some(join),
        })
    }

    /// Check now; the result arrives as `AppEvent::UpdateChecked`.
    pub fn check(&self) {
        self.send(Request::Check { manual: true });
    }

    /// Download and swap in `update`; the result arrives as
    /// `AppEvent::UpdateInstalled`.
    pub fn install(&self, update: Update) {
        self.send(Request::Install(update));
    }

    /// Stop offering `version` automatically.
    pub fn skip(&self, version: Version) {
        self.send(Request::Skip(version));
    }

    pub fn set_automatic(&self, on: bool) {
        self.send(Request::SetAutomatic(on));
    }

    fn send(&self, request: Request) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(request);
        }
    }

    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.tx = None;
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for UpdateWorker {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn update_main(
    rx: Receiver<Request>,
    state_path: PathBuf,
    automatic: bool,
    cancel: Arc<AtomicBool>,
    wake: Wake,
) {
    remove_leftovers();
    let started = unix_ms_now();
    let mut state = std::fs::read(&state_path)
        .map(|bytes| UpdateState::decode(&bytes))
        .unwrap_or_default();
    let mut automatic = automatic && !cfg!(debug_assertions);
    loop {
        // Asleep in `recv` between checks: no wakeups, no CPU (§1.4).
        let next = if automatic {
            rx.recv_timeout(state.next_check_in(unix_ms_now(), started))
        } else {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        let request = match next {
            Ok(request) => request,
            Err(RecvTimeoutError::Timeout) => Request::Check { manual: false },
            Err(RecvTimeoutError::Disconnected) => return,
        };
        match request {
            Request::Check { manual } => {
                let mut outcome = check(&cancel);
                if !manual {
                    state.last_check_ms = unix_ms_now();
                    state.last_check_failed = matches!(outcome, CheckOutcome::Failed(_));
                    save(&state_path, &state);
                    // A skipped version is offered again only when asked.
                    if let CheckOutcome::Available(update) = &outcome
                        && !state.offers(&update.version)
                    {
                        outcome = CheckOutcome::UpToDate;
                    }
                }
                wake.send(AppEvent::UpdateChecked { manual, outcome });
            }
            Request::Install(update) => {
                let result = install(&update, &cancel);
                wake.send(AppEvent::UpdateInstalled(result));
            }
            Request::Skip(version) => {
                state.skipped = Some(version.to_string());
                save(&state_path, &state);
            }
            Request::SetAutomatic(on) => automatic = on && !cfg!(debug_assertions),
        }
        if cancel.load(Ordering::Acquire) {
            return;
        }
    }
}

fn save(path: &Path, state: &UpdateState) {
    // A lost write only means one check too many.
    let _ = atomic_write(path, &state.encode());
}

fn check(cancel: &AtomicBool) -> CheckOutcome {
    let failed = |e: &dyn std::fmt::Display| {
        CheckOutcome::Failed(format!("Could not check for updates: {e}."))
    };
    match http::get(
        LATEST_RELEASE_API,
        "application/vnd.github+json",
        MAX_RELEASE_JSON_BYTES,
        cancel,
    ) {
        // No release published yet (or the repository is private).
        Err(HttpError::Status(404)) => CheckOutcome::UpToDate,
        Err(e) => failed(&e),
        Ok(json) => match update::newer_release(&json, &Version::current()) {
            Ok(Some(update)) => CheckOutcome::Available(update),
            Ok(None) => CheckOutcome::UpToDate,
            Err(e) => failed(&e),
        },
    }
}

/// The running executable, and the names its replacement passes through.
struct ExePaths {
    exe: PathBuf,
    /// The download, beside the exe so the swap is a same-volume rename.
    new: PathBuf,
    /// The running build, moved aside; deleted on the next start.
    old: PathBuf,
}

impl ExePaths {
    fn current() -> std::io::Result<Self> {
        let exe = std::env::current_exe()?;
        let with = |suffix: &str| {
            let mut name = exe.file_name().unwrap_or_default().to_owned();
            name.push(suffix);
            exe.with_file_name(name)
        };
        Ok(Self {
            new: with(".new"),
            old: with(".old"),
            exe,
        })
    }
}

fn remove_leftovers() {
    if let Ok(paths) = ExePaths::current() {
        let _ = std::fs::remove_file(&paths.old);
        let _ = std::fs::remove_file(&paths.new);
    }
}

/// Download, verify and swap in `update`. Returns the path to relaunch.
/// Any failure leaves the running build in place.
fn install(update: &Update, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let fail =
        |what: &str, e: &dyn std::fmt::Display| format!("Update not installed: {what} ({e}).");
    let paths = ExePaths::current().map_err(|e| fail("could not find smolcast.exe", &e))?;

    let checksum = http::get(
        &update.checksum.url,
        "application/octet-stream",
        4096,
        cancel,
    )
    .map_err(|e| fail("could not download the checksum", &e))?;
    let expected = std::str::from_utf8(&checksum)
        .ok()
        .and_then(update::parse_checksum)
        .ok_or_else(|| fail("the checksum file is malformed", &"no SHA-256"))?;
    let exe = http::get(
        &update.exe.url,
        "application/octet-stream",
        update.exe.size,
        cancel,
    )
    .map_err(|e| fail("could not download it", &e))?;
    if exe.len() as u64 != update.exe.size {
        return Err(fail("the download is incomplete", &"size mismatch"));
    }
    if sha256(&exe).map_err(|e| fail("could not hash the download", &e.message()))? != expected {
        return Err(fail("the download is corrupt", &"checksum mismatch"));
    }

    std::fs::write(&paths.new, &exe).map_err(|e| {
        fail(
            "smolcast's folder is not writable; download it from the release page",
            &e,
        )
    })?;
    let _ = std::fs::remove_file(&paths.old);
    if let Err(e) = std::fs::rename(&paths.exe, &paths.old) {
        let _ = std::fs::remove_file(&paths.new);
        return Err(fail("could not move the running build aside", &e));
    }
    if let Err(e) = std::fs::rename(&paths.new, &paths.exe) {
        // Put the running build back so the next start still finds it.
        let _ = std::fs::rename(&paths.old, &paths.exe);
        let _ = std::fs::remove_file(&paths.new);
        return Err(fail("could not put the new build in place", &e));
    }
    Ok(paths.exe)
}

fn sha256(bytes: &[u8]) -> windows::core::Result<[u8; 32]> {
    let mut digest = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, bytes, &mut digest) }.ok()?;
    Ok(digest)
}

/// Start the build at `exe` to replace this process, telling it to wait
/// until this process has exited.
pub fn relaunch(exe: &Path) -> std::io::Result<()> {
    std::process::Command::new(exe)
        .arg(WAIT_PID_FLAG)
        .arg(std::process::id().to_string())
        .spawn()
        .map(drop)
}

/// The update is installed but the new build did not start; say so, since
/// the palette and tray are already gone.
pub fn report_relaunch_failure(error: &std::io::Error) {
    let text = crate::win::wide(&format!(
        "smolcast was updated but could not restart ({error}). Start smolcast again."
    ));
    let caption = crate::win::wide("smolcast");
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}

/// A relaunched build waits here for the build it replaces to exit, so the
/// single-instance check does not mistake it for a second launch.
pub fn wait_for_previous_instance() {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg != WAIT_PID_FLAG {
            continue;
        }
        let Some(pid) = args.next().and_then(|pid| pid.parse::<u32>().ok()) else {
            return;
        };
        // Already gone: nothing to wait for.
        let Ok(process) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
            return;
        };
        // A timeout falls through to the single-instance check, which then
        // exits quietly if the old build is somehow still running.
        unsafe { WaitForSingleObject(process, WAIT_FOR_EXIT.as_millis() as u32) };
        let _ = unsafe { CloseHandle(process) };
        return;
    }
}
