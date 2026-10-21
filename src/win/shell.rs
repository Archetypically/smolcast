//! Windows shell work for the launcher (plan §1.4, §6): Start Menu +
//! AppsFolder enumeration, icon extraction, and shortcut/AUMID launch.
//!
//! Every job runs on its own thread inside a single-threaded apartment
//! (`COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE`), keeps COM objects
//! thread-local, converts results to owned values, and uninitializes COM before
//! the thread exits. Results reach the UI as `AppEvent`s through `Wake`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{PROPERTYKEY, SIZE};
use windows::Win32::Storage::FileSystem::SearchPathW;
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
    COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
    IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{
    AO_NONE, ApplicationActivationManager, BHID_EnumItems, FOF_ALLOWUNDO, FOF_NOCONFIRMATION,
    FOFX_RECYCLEONDELETE, FOLDERID_AppsFolder, FOLDERID_CommonPrograms, FOLDERID_Programs,
    FileOperation, IApplicationActivationManager, IEnumShellItems, IFileOperation, IShellItem,
    IShellItem2, IShellItemImageFactory, IShellLinkW, KF_FLAG_DEFAULT, SEE_MASK_FLAG_NO_UI,
    SEE_MASK_NOASYNC, SHCreateItemFromParsingName, SHELLEXECUTEINFOW, SHGetKnownFolderItem,
    SHGetKnownFolderPath, SIGDN_NORMALDISPLAY, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    ShellExecuteExW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{GUID, Interface, PCWSTR, PWSTR, w};

use crate::app::model::{AppEvent, Wake};
use crate::core::app_index::{
    AppId, AppSource, IndexedApp, LaunchSpec, RawEntry, ShortcutTarget, aumid_key, dedup,
    is_packaged_aumid, shortcut_id,
};
use crate::core::icon_scale;
use crate::win::icon::{IconRgba, OwnedBitmap, bitmap_to_rgba};

/// `System.AppUserModel.ID` ({9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}, 5).
const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};

/// Start Menu recursion bound; real trees are 2–4 levels deep.
const MAX_WALK_DEPTH: usize = 12;
/// `IShellLinkW` string buffers (UTF-16 units); covers long paths.
const LINK_BUF: usize = 32 * 1024;

/// A launcher failure. The message names the operation and app, never
/// clipboard or script content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchError {
    pub operation: &'static str,
    pub message: String,
    /// HRESULT or Win32 code when one exists.
    pub code: Option<u32>,
}

impl LaunchError {
    fn from_win(operation: &'static str, e: &windows::core::Error) -> Self {
        Self {
            operation,
            message: e.message(),
            code: Some(e.code().0 as u32),
        }
    }
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.operation, self.message)
    }
}

impl std::error::Error for LaunchError {}

/// A completed scan.
#[derive(Clone, Debug)]
pub struct ScanOutcome {
    pub apps: Vec<IndexedApp>,
    /// Shortcuts that failed to load or point at a missing file (§6.4).
    pub broken_shortcuts: Vec<BrokenShortcut>,
}

/// A Start Menu shortcut whose file does not load or whose target is gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokenShortcut {
    /// The file name without `.lnk`.
    pub name: String,
    pub path: PathBuf,
}

/// STA membership for the current thread; uninitializes on drop.
struct ComApartment;

impl ComApartment {
    fn enter() -> Result<Self, LaunchError> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
            .ok()
            .map_err(|e| LaunchError::from_win("initialize COM", &e))?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Take ownership of a COM-allocated string.
fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

fn from_buf(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

// ---- scan ---------------------------------------------------------------------

/// Start one index scan on its own thread (§1.4). The result arrives as
/// `AppEvent::IndexReady { generation, .. }`; `cancel` is checked between items.
pub fn spawn_scan(
    generation: u64,
    cancel: Arc<AtomicBool>,
    wake: Wake,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("smolcast-scan".into())
        .spawn(move || {
            let result = scan(&cancel);
            if !cancel.load(Ordering::Acquire) {
                wake.send(AppEvent::IndexReady { generation, result });
            }
        })
}

fn scan(cancel: &AtomicBool) -> Result<ScanOutcome, LaunchError> {
    let _com = ComApartment::enter()?;
    let mut entries = Vec::new();

    // AppsFolder first: shortcut identity depends on which AUMIDs are packaged.
    let packaged = enumerate_apps_folder(cancel)?;
    let packaged_keys: BTreeSet<String> =
        packaged.iter().map(|(aumid, _)| aumid_key(aumid)).collect();
    for (aumid, name) in packaged {
        entries.push(RawEntry {
            id: AppId::Packaged {
                aumid_key: aumid_key(&aumid),
            },
            display_name: name,
            source: AppSource::AppsFolder,
            icon_key: format!("shell:AppsFolder\\{aumid}"),
            launch: LaunchSpec::Packaged { aumid },
        });
    }

    let mut broken_shortcuts = Vec::new();
    for (folder, source) in [
        (FOLDERID_Programs, AppSource::UserStartMenu),
        (FOLDERID_CommonPrograms, AppSource::CommonStartMenu),
    ] {
        let root = known_folder_path(&folder)?;
        let mut links = Vec::new();
        collect_links(&root, 0, &mut links);
        links.sort();
        for lnk in links {
            if cancel.load(Ordering::Acquire) {
                return Err(LaunchError {
                    operation: "scan",
                    message: "cancelled".into(),
                    code: None,
                });
            }
            match read_shortcut(&lnk, source, &packaged_keys) {
                Ok(entry) => entries.push(entry),
                Err(Skip::Broken) => broken_shortcuts.push(BrokenShortcut {
                    name: lnk
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path: lnk,
                }),
                Err(Skip::Other) => {}
            }
        }
    }

    Ok(ScanOutcome {
        apps: dedup(entries),
        broken_shortcuts,
    })
}

fn known_folder_path(id: &GUID) -> Result<PathBuf, LaunchError> {
    let p = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }
        .map_err(|e| LaunchError::from_win("locate Start Menu", &e))?;
    Ok(PathBuf::from(take_pwstr(p)))
}

/// Recursively collect `.lnk` files. Symlinked directories are not followed.
fn collect_links(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_WALK_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            collect_links(&path, depth + 1, out);
        } else if kind.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
        {
            out.push(path);
        }
    }
}

/// Packaged AppsFolder rows as `(aumid, display name)`.
fn enumerate_apps_folder(cancel: &AtomicBool) -> Result<Vec<(String, String)>, LaunchError> {
    let folder: IShellItem =
        unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) }
            .map_err(|e| LaunchError::from_win("open AppsFolder", &e))?;
    let items: IEnumShellItems = unsafe { folder.BindToHandler(None, &BHID_EnumItems) }
        .map_err(|e| LaunchError::from_win("enumerate AppsFolder", &e))?;

    let mut out = Vec::new();
    loop {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let mut batch = [None];
        let mut fetched = 0u32;
        // A mid-list failure fails the scan, so the previous index is kept
        // rather than replaced by a partial one.
        unsafe { items.Next(&mut batch, Some(&mut fetched)) }
            .map_err(|e| LaunchError::from_win("enumerate AppsFolder", &e))?;
        if fetched == 0 {
            break;
        }
        let Some(item) = batch[0].take() else {
            break;
        };
        let Ok(item2) = item.cast::<IShellItem2>() else {
            continue;
        };
        let Ok(aumid) = unsafe { item2.GetString(&PKEY_APP_USER_MODEL_ID) }.map(take_pwstr) else {
            continue;
        };
        if !is_packaged_aumid(&aumid) {
            continue;
        }
        let name = unsafe { item.GetDisplayName(SIGDN_NORMALDISPLAY) }
            .map(take_pwstr)
            .unwrap_or_default();
        if !name.is_empty() {
            out.push((aumid, name));
        }
    }
    Ok(out)
}

/// Resolve one shortcut without triggering link tracking or search. `None`
/// means skip: unreadable, or its filesystem target no longer exists.
/// Why a shortcut was left out of the index.
enum Skip {
    /// The file does not load, or its local target is gone: safe to offer
    /// for removal.
    Broken,
    /// Anything else (COM trouble, no usable name, an offline drive): never
    /// offered for removal.
    Other,
}

fn read_shortcut(
    lnk: &Path,
    source: AppSource,
    packaged_keys: &BTreeSet<String>,
) -> Result<RawEntry, Skip> {
    let lnk_wide = wide_path(lnk);
    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .map_err(|_| Skip::Other)?;
    let file = link.cast::<IPersistFile>().map_err(|_| Skip::Other)?;
    // A file that reads but does not load is corrupt; one that cannot be read
    // right now (locked, access denied) is not called broken.
    unsafe { file.Load(PCWSTR(lnk_wide.as_ptr()), STGM_READ) }.map_err(|_| {
        if std::fs::read(lnk).is_ok() {
            Skip::Broken
        } else {
            Skip::Other
        }
    })?;

    let mut buf = vec![0u16; LINK_BUF];
    let target = unsafe { link.GetPath(&mut buf, std::ptr::null_mut(), 0) }
        .map(|()| from_buf(&buf))
        .unwrap_or_default();
    buf.fill(0);
    let arguments = unsafe { link.GetArguments(&mut buf) }
        .map(|()| from_buf(&buf))
        .unwrap_or_default();
    buf.fill(0);
    let working_dir = unsafe { link.GetWorkingDirectory(&mut buf) }
        .map(|()| from_buf(&buf))
        .unwrap_or_default();

    // A local absolute target that is gone is a broken shortcut (§6.4). UNC
    // targets are not probed: a sleeping share must not stall the scan. Bare
    // names (`pwsh.exe`) are resolved by the shell at launch, so they are not
    // probed either. Only a definite "not found" on an attached drive counts:
    // an access error or a detached drive (a USB stick) skips the shortcut
    // without calling it broken, since the app may still be there.
    let target_path = Path::new(&target);
    if target_path.is_absolute() && !target.starts_with("\\\\") {
        match target_path.try_exists() {
            Ok(true) => {}
            Ok(false) => {
                let drive_attached = target_path.ancestors().last().is_some_and(Path::exists);
                return Err(if drive_attached {
                    Skip::Broken
                } else {
                    Skip::Other
                });
            }
            Err(_) => return Err(Skip::Other),
        }
    }

    // Display name and AUMID come from the shell item, which honors localized
    // Start Menu names.
    let item: Option<IShellItem2> =
        unsafe { SHCreateItemFromParsingName(PCWSTR(lnk_wide.as_ptr()), None) }.ok();
    let display_name = item
        .as_ref()
        .and_then(|i| unsafe { i.GetDisplayName(SIGDN_NORMALDISPLAY) }.ok())
        .map(take_pwstr)
        .filter(|n| !n.is_empty())
        .or_else(|| lnk.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .ok_or(Skip::Other)?;
    let aumid = item
        .as_ref()
        .and_then(|i| unsafe { i.GetString(&PKEY_APP_USER_MODEL_ID) }.ok())
        .map(take_pwstr)
        .filter(|a| !a.is_empty());

    let lnk_str = lnk.to_string_lossy().into_owned();
    let resolved = ShortcutTarget {
        target,
        arguments,
        working_dir,
    };
    Ok(RawEntry {
        id: shortcut_id(&lnk_str, Some(&resolved), aumid.as_deref(), packaged_keys),
        display_name,
        source,
        launch: LaunchSpec::Shortcut {
            lnk_path: lnk.to_owned(),
        },
        icon_key: lnk_str,
    })
}

// ---- broken shortcuts ---------------------------------------------------------

/// Move shortcuts to the Recycle Bin on their own thread; the result arrives
/// as `AppEvent::ShortcutsRecycled`. Windows shows its own prompts, such as
/// administrator permission for all-users Start Menu shortcuts.
pub fn spawn_recycle(paths: Vec<PathBuf>, wake: Wake) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("smolcast-recycle".into())
        .spawn(move || {
            let result = recycle(&paths);
            wake.send(AppEvent::ShortcutsRecycled { result });
        })
}

/// Returns whether every item was moved; `false` when the user cancelled a
/// prompt or Windows skipped some.
fn recycle(paths: &[PathBuf]) -> Result<bool, LaunchError> {
    let _com = ComApartment::enter()?;
    let fail = |e: windows::core::Error| LaunchError::from_win("move to Recycle Bin", &e);
    let op: IFileOperation =
        unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }.map_err(fail)?;
    // Undo-able (Recycle Bin), and no second "are you sure": the palette
    // already listed the shortcuts and the user chose to remove them.
    unsafe { op.SetOperationFlags(FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOFX_RECYCLEONDELETE) }
        .map_err(fail)?;
    for path in paths {
        let wide = wide_path(path);
        let item: IShellItem =
            unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }.map_err(fail)?;
        unsafe { op.DeleteItem(&item, None) }.map_err(fail)?;
    }
    let performed = unsafe { op.PerformOperations() };
    let aborted = unsafe { op.GetAnyOperationsAborted() }.map_or(true, |b| b.as_bool());
    performed.map_err(fail)?;
    Ok(!aborted)
}

// ---- launch -------------------------------------------------------------------

/// Launch on its own thread; the result arrives as `AppEvent::LaunchFinished`.
/// Success means the shell accepted the launch (§6.3); smolcast does not wait
/// for a window.
pub fn spawn_launch(
    app_id: AppId,
    spec: LaunchSpec,
    wake: Wake,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("smolcast-launch".into())
        .spawn(move || {
            let result = launch(&spec);
            wake.send(AppEvent::LaunchFinished { app_id, result });
        })
}

fn launch(spec: &LaunchSpec) -> Result<(), LaunchError> {
    let _com = ComApartment::enter()?;
    match spec {
        LaunchSpec::Shortcut { lnk_path } => {
            // The original .lnk goes to the shell so its arguments and working
            // directory apply exactly as Explorer would apply them.
            let file = wide_path(lnk_path);
            let mut info = SHELLEXECUTEINFOW {
                cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
                lpFile: PCWSTR(file.as_ptr()),
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            unsafe { ShellExecuteExW(&mut info) }
                .map_err(|e| LaunchError::from_win("launch shortcut", &e))
        }
        LaunchSpec::Script(script) => crate::win::process::spawn_script(script),
        LaunchSpec::Command(_) => Err(LaunchError {
            operation: "launch",
            message: "built-in commands open in the palette".into(),
            code: None,
        }),
        LaunchSpec::Packaged { aumid } => {
            let manager: IApplicationActivationManager = unsafe {
                CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
            }
            .map_err(|e| LaunchError::from_win("activate app", &e))?;
            let id = wide(aumid);
            unsafe { manager.ActivateApplication(PCWSTR(id.as_ptr()), PCWSTR::null(), AO_NONE) }
                .map(|_pid| ())
                .map_err(|e| LaunchError::from_win("activate app", &e))
        }
    }
}

// ---- icons --------------------------------------------------------------------

struct IconRequest {
    app_id: AppId,
    icon_key: String,
    size: u32,
}

/// One long-lived STA thread that extracts icons on request (§6.4). Requests
/// are processed in order; the caller only asks for visible rows and caches
/// results, so the queue stays short.
pub struct IconWorker {
    tx: Option<Sender<IconRequest>>,
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl IconWorker {
    pub fn start(wake: Wake) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let join = thread::Builder::new()
            .name("smolcast-icons".into())
            .spawn(move || icon_main(rx, worker_cancel, wake))?;
        Ok(Self {
            tx: Some(tx),
            cancel,
            join: Some(join),
        })
    }

    pub fn request(&self, app_id: AppId, icon_key: String, size: u32) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(IconRequest {
                app_id,
                icon_key,
                size,
            });
        }
    }

    /// Drop queued work, stop, and join (§1.5 step 5).
    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.tx = None; // closing the channel ends the worker's recv loop
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for IconWorker {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn icon_main(rx: Receiver<IconRequest>, cancel: Arc<AtomicBool>, wake: Wake) {
    let com = ComApartment::enter();
    while let Ok(request) = rx.recv() {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let result = match &com {
            Ok(_) => extract_icon(&request.icon_key, request.size),
            Err(e) => Err(e.clone()),
        };
        wake.send(AppEvent::IconReady {
            app_id: request.app_id,
            icon_key: request.icon_key,
            result,
        });
    }
}

/// `icon_key` is a shell parsing name: a `.lnk` path, `shell:AppsFolder\<AUMID>`,
/// or a script interpreter, which may be a bare name such as `pwsh.exe`.
fn extract_icon(icon_key: &str, size: u32) -> Result<IconRgba, LaunchError> {
    let key = resolve_executable(icon_key).unwrap_or_else(|| wide(icon_key));
    let factory: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(PCWSTR(key.as_ptr()), None) }
            .map_err(|e| LaunchError::from_win("load icon", &e))?;
    // The shell's own resize to odd edges is coarse; fetch a stock size and
    // area-average down to the drawn pixel size.
    let edge = icon_scale::fetch_edge(size) as i32;
    let bitmap = unsafe {
        factory.GetImage(
            SIZE { cx: edge, cy: edge },
            SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
        )
    }
    .map_err(|e| LaunchError::from_win("load icon", &e))?;
    let bitmap = OwnedBitmap(bitmap);
    let icon = bitmap_to_rgba(&bitmap).ok_or(LaunchError {
        operation: "load icon",
        message: "unsupported icon bitmap".into(),
        code: None,
    })?;
    let (width, height, trimmed) =
        icon_scale::trim_padding(icon.width, icon.height, &icon.rgba_premultiplied);
    let (width, height, rgba_premultiplied) =
        icon_scale::downscale_premultiplied(width, height, &trimmed, size);
    Ok(IconRgba {
        width,
        height,
        rgba_premultiplied,
    })
}

/// Full path of a bare executable name, found the way `SearchPathW` looks
/// (application directory, system directories, then `PATH`), adding `.exe`
/// when the name has no extension. Paths and shell names are left alone.
fn resolve_executable(name: &str) -> Option<Vec<u16>> {
    if name.contains(['\\', '/', ':']) {
        return None;
    }
    let file = wide(name);
    let mut buffer = vec![0u16; 1024];
    let len = unsafe {
        SearchPathW(
            PCWSTR::null(),
            PCWSTR(file.as_ptr()),
            w!(".exe"),
            Some(&mut buffer),
            None,
        )
    } as usize;
    if len == 0 || len >= buffer.len() {
        return None;
    }
    buffer.truncate(len + 1); // keep the terminator `PCWSTR` needs
    Some(buffer)
}

/// Open a URI (`ms-settings:clipboard`, or a web search in the default
/// browser) with the shell, off the UI thread. Failures are ignored: there is
/// nothing smolcast could do about a missing browser or Settings app.
pub fn open_uri(uri: impl Into<String>) {
    let uri = uri.into();
    let _ = thread::Builder::new()
        .name("smolcast-open".into())
        .spawn(move || {
            let Ok(_com) = ComApartment::enter() else {
                return;
            };
            let file = wide(&uri);
            let mut info = SHELLEXECUTEINFOW {
                cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
                lpFile: PCWSTR(file.as_ptr()),
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            let _ = unsafe { ShellExecuteExW(&mut info) };
        });
}
