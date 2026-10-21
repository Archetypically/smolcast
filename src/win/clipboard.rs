//! Clipboard worker thread (plan §1.3, §7.1–§7.4).
//!
//! One dedicated thread owns one message-only HWND and every clipboard read
//! and write: the format listener, contention retries, exclusion and
//! self-write markers, and conversion to owned payloads. The UI sends
//! `ClipboardCommand`s and wakes the loop with `WAKE_COMMAND`; results return
//! as `AppEvent`s through `Wake`.
//!
//! Loop rules: nothing sleeps. A failed `OpenClipboard` arms a one-shot timer
//! (5, 10, 25, 50, 100 ms). Writes run before captures. Update notifications
//! coalesce to the newest sequence number. Every successful `OpenClipboard`
//! is paired with exactly one `CloseClipboard` by a guard, and handles from
//! `GetClipboardData` are only locked, copied, and unlocked, never freed.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use windows::ApplicationModel::DataTransfer::{
    Clipboard, ClipboardHistoryItem, ClipboardHistoryItemsResultStatus,
    SetHistoryItemAsContentStatus, StandardDataFormats,
};
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform,
    ColorManagementMode, ExifOrientationMode,
};
use windows::Win32::Foundation::{
    GlobalFree, HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    KillTimer, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer, TranslateMessage,
    UnregisterClassW, WINDOW_EX_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WM_DESTROY, WM_TIMER, WNDCLASSW,
    WS_OVERLAPPED,
};
use windows::core::PCWSTR;

use crate::app::model::{AppEvent, Wake};
use crate::app::notice::{Notice, Severity, op};
use crate::core::config::ClipboardSource;
use crate::core::dib::{DibError, ImagePayload, decode_dib, encode_dib24, encode_dibv5};
use crate::core::history::{Payload, TextError, decode_unicode_text, encode_unicode_text};

/// Commands sent to the clipboard thread (§7.1).
#[derive(Debug)]
pub enum ClipboardCommand {
    SetText {
        request_id: u64,
        text: String,
    },
    SetImage {
        request_id: u64,
        image: ImagePayload,
    },
    /// Which history is live (§7.0) and whether config allows smolcast's own
    /// capture. In `Auto` the thread captures only while Windows history is off.
    SetCapturePolicy {
        source: ClipboardSource,
        capture: bool,
    },
    /// Per-item byte cap after a config reload (§7.5).
    SetItemCap(usize),
    /// Snapshot Windows clipboard history (§7.0).
    WindowsRefresh {
        generation: u64,
    },
    /// Paste-back step 1 for a Windows history item.
    WindowsSetContent {
        request_id: u64,
        item_id: String,
    },
    WindowsDelete {
        item_id: String,
    },
    WindowsClear,
    Shutdown,
}

/// Owned view of one Windows clipboard history item (§7.0).
#[derive(Clone, Debug)]
pub struct WindowsItem {
    pub id: String,
    pub captured_at_ms: i64,
    pub content: WindowsContent,
}

#[derive(Clone, Debug)]
pub enum WindowsContent {
    /// At most `WINDOWS_TEXT_PREVIEW` bytes of the text.
    Text(String),
    /// Original size plus a straight-alpha RGBA thumbnail (≤256 px).
    Image {
        width: u32,
        height: u32,
        thumbnail: ImagePayload,
    },
    /// Neither text nor bitmap (for example files); pasteable but not previewed.
    Other,
}

/// Result of a Windows history refresh.
#[derive(Clone, Debug)]
pub struct WindowsSnapshot {
    /// False when Windows clipboard history is turned off in Settings.
    pub enabled: bool,
    /// Newest first.
    pub items: Vec<WindowsItem>,
}

/// A completed smolcast write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardWriteReceipt {
    /// `GetClipboardSequenceNumber` right after the write closed.
    pub sequence: u32,
}

/// A content-free clipboard failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardError {
    pub operation: &'static str,
    pub message: String,
    pub code: Option<u32>,
}

impl ClipboardError {
    fn new(operation: &'static str, message: impl Into<String>) -> Self {
        Self {
            operation,
            message: message.into(),
            code: None,
        }
    }

    fn from_win(operation: &'static str, e: &windows::core::Error) -> Self {
        Self {
            operation,
            message: e.message(),
            code: Some(e.code().0 as u32),
        }
    }
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.operation, self.message)
    }
}

impl std::error::Error for ClipboardError {}

/// `WM_APP + 1` wakes the message loop so it drains queued commands (§1.3).
const WAKE_COMMAND: u32 = WM_APP + 1;
const RETRY_TIMER: usize = 1;
/// One-shot retry delays after a failed `OpenClipboard` (§7.1).
const RETRY_DELAYS_MS: [u32; 5] = [5, 10, 25, 50, 100];

/// Text preview bytes kept per Windows history item (§7.0).
const WINDOWS_TEXT_PREVIEW: usize = 64 * 1024;
const WINDOWS_THUMBNAIL_EDGE: u32 = 256;
/// 100 ns intervals between 1601-01-01 and 1970-01-01.
const FILETIME_UNIX_EPOCH: i64 = 116_444_736_000_000_000;

const CF_DIB: u32 = 8;
/// Allowance over the item cap for a raw DIB's metadata.
const DIB_METADATA_SLACK: usize = 2048;
const CF_UNICODETEXT: u32 = 13;
const CF_DIBV5: u32 = 17;

thread_local! {
    /// Set by the window procedure; consumed by the loop after each dispatch.
    static UPDATE_PENDING: Cell<bool> = const { Cell::new(false) };
    static TIMER_FIRED: Cell<bool> = const { Cell::new(false) };
}

/// Handle to the clipboard worker. `shutdown` performs the ordered teardown
/// (§1.3): remove the listener, kill timers, destroy the HWND, unregister the
/// class, post quit, and join.
pub struct ClipboardThread {
    command_tx: Sender<ClipboardCommand>,
    hwnd: HWND,
    join: Option<JoinHandle<()>>,
}

/// The ready handshake crosses a thread boundary, and `HWND` (a raw pointer) is
/// not `Send`; the integer value is moved instead and rebuilt on this side.
type ReadyValue = Result<isize, ClipboardError>;

impl ClipboardThread {
    /// Spawn the thread and block on the ready handshake, which is sent only
    /// after the class, HWND, and format listener exist (§1.3).
    pub fn start(
        item_cap: usize,
        source: ClipboardSource,
        capture: bool,
        wake: Wake,
    ) -> Result<Self, ClipboardError> {
        let (command_tx, command_rx) = mpsc::channel::<ClipboardCommand>();
        let (ready_tx, ready_rx) = mpsc::channel::<ReadyValue>();
        let join = thread::Builder::new()
            .name("smolcast-clipboard".into())
            .spawn(move || {
                clipboard_main(
                    command_rx,
                    ready_tx,
                    Worker::new(item_cap, source, capture, wake),
                )
            })
            .map_err(|e| ClipboardError::new("spawn clipboard thread", e.to_string()))?;

        let raw = ready_rx.recv().map_err(|e| {
            ClipboardError::new("start clipboard thread", format!("died before ready: {e}"))
        })??;
        let hwnd = HWND(raw as *mut core::ffi::c_void);

        Ok(Self {
            command_tx,
            hwnd,
            join: Some(join),
        })
    }

    /// Queue a command and wake the message loop (§1.3).
    pub fn send_command(&self, command: ClipboardCommand) {
        if self.command_tx.send(command).is_ok() {
            let _ = unsafe { PostMessageW(Some(self.hwnd), WAKE_COMMAND, WPARAM(0), LPARAM(0)) };
        }
    }

    /// Ordered shutdown: enqueue `Shutdown`, wake, join (§1.5 step 4).
    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = self.command_tx.send(ClipboardCommand::Shutdown);
            let _ = unsafe { PostMessageW(Some(self.hwnd), WAKE_COMMAND, WPARAM(0), LPARAM(0)) };
            let _ = join.join();
        }
    }
}

impl Drop for ClipboardThread {
    /// Safety net: on abnormal teardown (for example an app panic that skips
    /// `on_exit`), still stop the thread and destroy its message-only window.
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn clipboard_main(
    command_rx: Receiver<ClipboardCommand>,
    ready: Sender<ReadyValue>,
    worker: Worker,
) {
    // An STA with a message pump owns every WinRT clipboard-history object (§7.0).
    if let Err(e) = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok() {
        let _ = ready.send(Err(ClipboardError::from_win("initialize COM", &e)));
        return;
    }
    run_clipboard_thread(command_rx, ready, worker);
    unsafe { CoUninitialize() };
}

fn run_clipboard_thread(
    command_rx: Receiver<ClipboardCommand>,
    ready: Sender<ReadyValue>,
    mut worker: Worker,
) {
    let class_wide = wide(&format!("smolcast-clip-{}", unsafe {
        GetCurrentProcessId()
    }));

    let hinstance = match unsafe { GetModuleHandleW(None) } {
        Ok(module) => HINSTANCE(module.0),
        Err(e) => {
            let _ = ready.send(Err(ClipboardError::from_win("GetModuleHandleW", &e)));
            return;
        }
    };

    let class = WNDCLASSW {
        lpfnWndProc: Some(clipboard_wnd_proc),
        lpszClassName: PCWSTR(class_wide.as_ptr()),
        hInstance: hinstance,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        let _ = ready.send(Err(ClipboardError::new(
            "RegisterClassW",
            std::io::Error::last_os_error().to_string(),
        )));
        return;
    }
    let unregister = || {
        let _ = unsafe { UnregisterClassW(PCWSTR(class_wide.as_ptr()), Some(hinstance)) };
    };

    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_wide.as_ptr()),
            PCWSTR::null(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        )
    } {
        Ok(h) if !h.is_invalid() => h,
        other => {
            let _ = ready.send(Err(match other {
                Ok(_) => ClipboardError::new("CreateWindowExW", "invalid HWND"),
                Err(e) => ClipboardError::from_win("CreateWindowExW", &e),
            }));
            unregister();
            return;
        }
    };

    // Milestone 3 onward: the listener exists before ready is sent (§1.3).
    if let Err(e) = unsafe { AddClipboardFormatListener(hwnd) } {
        let _ = ready.send(Err(ClipboardError::from_win(
            "AddClipboardFormatListener",
            &e,
        )));
        let _ = unsafe { DestroyWindow(hwnd) };
        unregister();
        return;
    }
    worker.hwnd = hwnd;
    worker.formats = Formats::register();

    if ready.send(Ok(hwnd.0 as isize)).is_err() {
        // The UI side died before the handshake; run a minimal teardown.
        let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
        let _ = unsafe { DestroyWindow(hwnd) };
        unregister();
        return;
    }

    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if ret.0 <= 0 {
            break; // 0 = WM_QUIT, negative = error
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if worker.drain_commands(&command_rx) {
            break;
        }
        if UPDATE_PENDING.replace(false) {
            worker.pending_capture = true;
        }
        if TIMER_FIRED.replace(false) {
            let _ = unsafe { KillTimer(Some(hwnd), RETRY_TIMER) };
            worker.timer_armed = false;
        }
        worker.advance();
    }

    let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
    let _ = unsafe { KillTimer(Some(hwnd), RETRY_TIMER) };
    let _ = unsafe { DestroyWindow(hwnd) };
    unregister();
    // WinRT objects are released on this thread, before COM uninitializes.
    drop(worker);
}

unsafe extern "system" fn clipboard_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // The loop drains commands after every dispatch; the wake only
        // unblocks `GetMessageW`.
        WAKE_COMMAND => LRESULT(0),
        WM_CLIPBOARDUPDATE => {
            UPDATE_PENDING.set(true);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == RETRY_TIMER => {
            TIMER_FIRED.set(true);
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Registered format IDs (§7.2); 0 when registration failed.
#[derive(Default)]
struct Formats {
    self_write: u32,
    exclude_monitor: u32,
    history_allowed: u32,
}

impl Formats {
    fn register() -> Self {
        let register =
            |name: &str| unsafe { RegisterClipboardFormatW(PCWSTR(wide(name).as_ptr())) };
        Self {
            self_write: register("smolcast.self-write.v1"),
            exclude_monitor: register("ExcludeClipboardContentFromMonitorProcessing"),
            history_allowed: register("CanIncludeInClipboardHistory"),
        }
    }
}

enum Write {
    Text {
        request_id: u64,
        text: String,
    },
    Image {
        request_id: u64,
        image: ImagePayload,
    },
}

impl Write {
    fn request_id(&self) -> u64 {
        match self {
            Write::Text { request_id, .. } | Write::Image { request_id, .. } => *request_id,
        }
    }
}

/// Loop-owned worker state.
struct Worker {
    hwnd: HWND,
    formats: Formats,
    wake: Wake,
    item_cap: usize,
    source: ClipboardSource,
    capture: bool,
    writes: VecDeque<Write>,
    /// Items from the latest Windows history snapshot, by id.
    windows_items: HashMap<String, ClipboardHistoryItem>,
    /// Converted content by item id; items are immutable, so this survives refreshes.
    windows_content: HashMap<String, WindowsContent>,
    pending_capture: bool,
    /// Sequence number right after our last write; that update is not captured.
    self_write_sequence: Option<u32>,
    last_captured_sequence: Option<u32>,
    /// Failed `OpenClipboard` attempts for the operation at the head.
    attempts: usize,
    timer_armed: bool,
}

enum Attempt {
    Done,
    Busy,
}

impl Worker {
    fn new(item_cap: usize, source: ClipboardSource, capture: bool, wake: Wake) -> Self {
        Self {
            hwnd: HWND::default(),
            formats: Formats::default(),
            wake,
            item_cap,
            source,
            capture,
            writes: VecDeque::new(),
            windows_items: HashMap::new(),
            windows_content: HashMap::new(),
            pending_capture: false,
            self_write_sequence: None,
            last_captured_sequence: None,
            attempts: 0,
            timer_armed: false,
        }
    }

    /// Returns true when `Shutdown` was received.
    fn drain_commands(&mut self, rx: &Receiver<ClipboardCommand>) -> bool {
        let mut stop = false;
        for command in rx.try_iter() {
            match command {
                ClipboardCommand::SetText { request_id, text } => {
                    self.queue_write(Write::Text { request_id, text });
                }
                ClipboardCommand::SetImage { request_id, image } => {
                    self.queue_write(Write::Image { request_id, image });
                }
                ClipboardCommand::SetCapturePolicy { source, capture } => {
                    self.source = source;
                    self.capture = capture;
                }
                ClipboardCommand::WindowsRefresh { generation } => self.windows_refresh(generation),
                ClipboardCommand::WindowsSetContent {
                    request_id,
                    item_id,
                } => self.windows_set_content(request_id, &item_id),
                ClipboardCommand::WindowsDelete { item_id } => {
                    if let Some(item) = self.windows_items.remove(&item_id) {
                        let _ = Clipboard::DeleteItemFromHistory(&item);
                    }
                    self.windows_content.remove(&item_id);
                }
                ClipboardCommand::WindowsClear => {
                    let _ = Clipboard::ClearHistory();
                }
                ClipboardCommand::SetItemCap(cap) => self.item_cap = cap,
                ClipboardCommand::Shutdown => stop = true,
            }
        }
        stop
    }

    fn queue_write(&mut self, write: Write) {
        if self.writes.is_empty() {
            // A write preempts a capture that is waiting on contention.
            self.attempts = 0;
        }
        self.writes.push_back(write);
    }

    /// Run as much queued work as the clipboard allows without waiting.
    fn advance(&mut self) {
        if self.timer_armed {
            return; // a retry is scheduled; its WM_TIMER resumes the work
        }
        loop {
            if let Some(write) = self.writes.pop_front() {
                if let Attempt::Busy = self.try_write(&write) {
                    self.writes.push_front(write);
                    return;
                }
                self.attempts = 0;
                continue;
            }
            if self.pending_capture {
                if !self.should_capture() {
                    self.pending_capture = false;
                    // The Windows history view refreshes on this signal.
                    self.wake.send(AppEvent::ClipboardChanged);
                    return;
                }
                if let Attempt::Busy = self.try_capture() {
                    return;
                }
                self.pending_capture = false;
                self.attempts = 0;
            }
            return;
        }
    }

    /// smolcast's own history captures only when it is the effective source.
    fn should_capture(&self) -> bool {
        self.capture
            && match self.source {
                ClipboardSource::Smolcast => true,
                ClipboardSource::Windows => false,
                ClipboardSource::Auto => !windows_history_enabled(),
            }
    }

    fn windows_refresh(&mut self, generation: u64) {
        let result = self.windows_snapshot();
        self.wake
            .send(AppEvent::WindowsHistory { generation, result });
    }

    fn windows_snapshot(&mut self) -> Result<WindowsSnapshot, ClipboardError> {
        let win =
            |op: &'static str| move |e: windows::core::Error| ClipboardError::from_win(op, &e);
        let result = Clipboard::GetHistoryItemsAsync()
            .and_then(|op| op.join())
            .map_err(win("read Windows clipboard history"))?;
        match result
            .Status()
            .map_err(win("read Windows clipboard history"))?
        {
            ClipboardHistoryItemsResultStatus::Success => {}
            ClipboardHistoryItemsResultStatus::ClipboardHistoryDisabled => {
                self.windows_items.clear();
                return Ok(WindowsSnapshot {
                    enabled: false,
                    items: Vec::new(),
                });
            }
            _ => {
                return Err(ClipboardError::new(
                    "read Windows clipboard history",
                    "access denied",
                ));
            }
        }
        let items = result
            .Items()
            .map_err(win("read Windows clipboard history"))?;
        let mut snapshot = Vec::new();
        let mut live = HashMap::new();
        for item in &items {
            let Ok(id) = item.Id().map(|h| h.to_string_lossy()) else {
                continue;
            };
            let captured_at_ms = item
                .Timestamp()
                .map(|t| (t.UniversalTime - FILETIME_UNIX_EPOCH) / 10_000)
                .unwrap_or(0);
            let content = match self.windows_content.get(&id) {
                Some(content) => content.clone(),
                None => windows_content(&item),
            };
            self.windows_content.insert(id.clone(), content.clone());
            snapshot.push(WindowsItem {
                id: id.clone(),
                captured_at_ms,
                content,
            });
            live.insert(id, item);
        }
        self.windows_content.retain(|id, _| live.contains_key(id));
        self.windows_items = live;
        Ok(WindowsSnapshot {
            enabled: true,
            items: snapshot,
        })
    }

    /// Paste-back step 1 for a Windows item (§7.0, §8.2).
    fn windows_set_content(&mut self, request_id: u64, item_id: &str) {
        let result = match self.windows_items.get(item_id) {
            None => Err(ClipboardError::new(
                "paste from Windows history",
                "the item is no longer in clipboard history",
            )),
            Some(item) => match Clipboard::SetHistoryItemAsContent(item) {
                Ok(SetHistoryItemAsContentStatus::Success) => Ok(ClipboardWriteReceipt {
                    sequence: unsafe { GetClipboardSequenceNumber() },
                }),
                Ok(_) => Err(ClipboardError::new(
                    "paste from Windows history",
                    "Windows refused to restore the item",
                )),
                Err(e) => Err(ClipboardError::from_win("paste from Windows history", &e)),
            },
        };
        self.finish_write(request_id, result);
    }

    /// Open the clipboard or schedule a retry. `None` means give up.
    fn open(&mut self) -> Option<Result<ClipboardOpen, ()>> {
        match unsafe { OpenClipboard(Some(self.hwnd)) } {
            Ok(()) => Some(Ok(ClipboardOpen)),
            Err(_) if self.attempts < RETRY_DELAYS_MS.len() => {
                let delay = RETRY_DELAYS_MS[self.attempts];
                self.attempts += 1;
                // Without a timer no retry would ever arrive and the queue
                // would stall; fail this operation instead.
                if unsafe { SetTimer(Some(self.hwnd), RETRY_TIMER, delay, None) } == 0 {
                    return Some(Err(()));
                }
                self.timer_armed = true;
                None
            }
            Err(_) => Some(Err(())),
        }
    }

    /// `Done` covers both a finished write and a terminal failure; both send
    /// `ClipboardWriteFinished`.
    fn try_write(&mut self, write: &Write) -> Attempt {
        let request_id = write.request_id();
        let guard = match self.open() {
            None => return Attempt::Busy,
            Some(Ok(guard)) => guard,
            Some(Err(())) => {
                self.finish_write(
                    request_id,
                    Err(ClipboardError::new(
                        "write clipboard",
                        "the clipboard stayed busy; try again",
                    )),
                );
                return Attempt::Done;
            }
        };
        let result = self.write_open(write);
        drop(guard);
        let result = result.map(|()| {
            // Secondary self-write guard: the exact post-close sequence (§7.2).
            let sequence = unsafe { GetClipboardSequenceNumber() };
            self.self_write_sequence = Some(sequence);
            ClipboardWriteReceipt { sequence }
        });
        self.finish_write(request_id, result);
        Attempt::Done
    }

    fn finish_write(&self, request_id: u64, result: Result<ClipboardWriteReceipt, ClipboardError>) {
        self.wake
            .send(AppEvent::ClipboardWriteFinished { request_id, result });
    }

    /// Empty, publish the content, then the self-write marker, all inside one
    /// open transaction (§7.2).
    fn write_open(&self, write: &Write) -> Result<(), ClipboardError> {
        unsafe { EmptyClipboard() }.map_err(|e| ClipboardError::from_win("empty clipboard", &e))?;
        match write {
            Write::Text { text, .. } => publish(CF_UNICODETEXT, &encode_unicode_text(text))?,
            Write::Image { image, .. } => {
                // Explicit 24-bit CF_DIB for alpha-unaware consumers; CF_DIBV5
                // carries alpha for those that read it (§7.4).
                publish(CF_DIB, &encode_dib24(image))?;
                publish(CF_DIBV5, &encode_dibv5(image))?;
            }
        }
        if self.formats.self_write != 0 {
            // A missing marker is tolerated: the sequence guard still applies.
            let _ = publish(self.formats.self_write, &1u32.to_le_bytes());
        }
        Ok(())
    }

    fn try_capture(&mut self) -> Attempt {
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if Some(sequence) == self.self_write_sequence
            || Some(sequence) == self.last_captured_sequence
        {
            return Attempt::Done;
        }
        if self.format_present(self.formats.self_write)
            || self.format_present(self.formats.exclude_monitor)
        {
            self.last_captured_sequence = Some(sequence);
            return Attempt::Done;
        }
        let guard = match self.open() {
            None => return Attempt::Busy,
            Some(Ok(guard)) => guard,
            // Terminal contention: this sequence is skipped (§7.1).
            Some(Err(())) => return Attempt::Done,
        };
        let raw = self.read_open();
        drop(guard); // decode only after the clipboard is closed (§7.4)
        self.last_captured_sequence = Some(sequence);

        // Oversize items are skipped with a content-free notice (§7.5);
        // undecodable ones are skipped silently.
        let payload = match raw {
            Some(Raw::Text(bytes)) => match decode_unicode_text(&bytes, self.item_cap) {
                Ok(text) => Some(Payload::Text(text)),
                Err(TextError::TooLarge) => return self.skipped("text", bytes.len() / 2),
                Err(_) => None,
            },
            Some(Raw::Dib(bytes)) => match decode_dib(&bytes, self.item_cap) {
                Ok(image) => Some(Payload::Image(image)),
                Err(DibError::TooLarge { bytes }) => {
                    return self.skipped("image", usize::try_from(bytes).unwrap_or(usize::MAX));
                }
                Err(_) => None,
            },
            Some(Raw::TooLarge { kind, bytes }) => return self.skipped(kind, bytes),
            None => return Attempt::Done,
        };
        if let Some(payload) = payload {
            self.wake.send(AppEvent::ClipboardCaptured(payload));
        }
        Attempt::Done
    }

    fn skipped(&self, kind: &str, bytes: usize) -> Attempt {
        let mib = |b: usize| b as f64 / (1024.0 * 1024.0);
        self.wake.send(AppEvent::Notice(Notice::new(
            Severity::Info,
            op::CLIPBOARD,
            format!(
                "Skipped a {:.1} MiB clipboard {kind}: over the {:.1} MiB item limit \
                 (clipboard_max_item_bytes).",
                mib(bytes),
                mib(self.item_cap)
            ),
        )));
        Attempt::Done
    }

    fn format_present(&self, format: u32) -> bool {
        format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
    }

    /// Copy the preferred format out while the clipboard is open. Text wins over
    /// images; `CF_DIBV5` is preferred to `CF_DIB`.
    fn read_open(&self) -> Option<Raw> {
        if self.formats.history_allowed != 0
            && let Copied::Bytes(flag) = copy_format(self.formats.history_allowed, 16)
            && flag.len() >= 4
            && u32::from_le_bytes(flag[..4].try_into().expect("4 bytes")) == 0
        {
            return None; // CanIncludeInClipboardHistory = 0 (§7.2)
        }
        let text_cap = self
            .item_cap
            .checked_mul(2)
            .and_then(|n| n.checked_add(2))?;
        if self.format_present(CF_UNICODETEXT) {
            return match copy_format(CF_UNICODETEXT, text_cap) {
                Copied::Bytes(bytes) => Some(Raw::Text(bytes)),
                Copied::TooLarge(bytes) => Some(Raw::TooLarge {
                    kind: "text",
                    bytes: bytes / 2,
                }),
                Copied::Missing => None,
            };
        }
        // The raw block also holds the header, masks, and palette (at most
        // 124 + 12 + 1024 bytes); decoding enforces the RGBA cap exactly.
        let dib_cap = self.item_cap.saturating_add(DIB_METADATA_SLACK);
        for format in [CF_DIBV5, CF_DIB] {
            if self.format_present(format) {
                return match copy_format(format, dib_cap) {
                    Copied::Bytes(bytes) => Some(Raw::Dib(bytes)),
                    Copied::TooLarge(bytes) => Some(Raw::TooLarge {
                        kind: "image",
                        bytes,
                    }),
                    Copied::Missing => None,
                };
            }
        }
        None
    }
}

enum Raw {
    Text(Vec<u8>),
    Dib(Vec<u8>),
    /// Over the pre-copy bound; nothing was copied.
    TooLarge {
        kind: &'static str,
        bytes: usize,
    },
}

enum Copied {
    Bytes(Vec<u8>),
    TooLarge(usize),
    Missing,
}

/// Proof of an open clipboard; closes it exactly once.
struct ClipboardOpen;

impl Drop for ClipboardOpen {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// Copy a borrowed clipboard handle's bytes. A block larger than
/// `max_bytes` is reported by size without copying.
fn copy_format(format: u32, max_bytes: usize) -> Copied {
    let Ok(handle) = (unsafe { GetClipboardData(format) }) else {
        return Copied::Missing;
    };
    let hglobal = HGLOBAL(handle.0);
    let size = unsafe { GlobalSize(hglobal) };
    if size == 0 {
        return Copied::Missing;
    }
    if size > max_bytes {
        return Copied::TooLarge(size);
    }
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        return Copied::Missing;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, size) }.to_vec();
    let _ = unsafe { GlobalUnlock(hglobal) };
    Copied::Bytes(bytes)
}

/// Publish `bytes` as `format` in a new `GMEM_MOVEABLE` block. On success the
/// system owns the block; on failure it is freed here.
fn publish(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
    let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }
        .map_err(|e| ClipboardError::from_win("allocate clipboard memory", &e))?;
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        let _ = unsafe { GlobalFree(Some(hglobal)) };
        return Err(ClipboardError::new(
            "lock clipboard memory",
            "GlobalLock failed",
        ));
    }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len()) };
    let _ = unsafe { GlobalUnlock(hglobal) };
    match unsafe { SetClipboardData(format, Some(HANDLE(hglobal.0))) } {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = unsafe { GlobalFree(Some(hglobal)) };
            Err(ClipboardError::from_win("set clipboard data", &e))
        }
    }
}

fn windows_history_enabled() -> bool {
    Clipboard::IsHistoryEnabled().unwrap_or(false)
}

/// Convert one history item's content to owned data. Failures degrade to
/// `Other`, which still pastes.
fn windows_content(item: &ClipboardHistoryItem) -> WindowsContent {
    let Ok(view) = item.Content() else {
        return WindowsContent::Other;
    };
    let has = |format: windows::core::Result<windows::core::HSTRING>| {
        format.and_then(|f| view.Contains(&f)).unwrap_or(false)
    };
    if has(StandardDataFormats::Text())
        && let Ok(text) = view.GetTextAsync().and_then(|op| op.join())
    {
        let text = text.to_string_lossy();
        let mut end = text.len().min(WINDOWS_TEXT_PREVIEW);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        return WindowsContent::Text(text[..end].to_owned());
    }
    if has(StandardDataFormats::Bitmap())
        && let Ok(image) = windows_thumbnail(&view)
    {
        return image;
    }
    WindowsContent::Other
}

fn windows_thumbnail(
    view: &windows::ApplicationModel::DataTransfer::DataPackageView,
) -> windows::core::Result<WindowsContent> {
    let stream = view.GetBitmapAsync()?.join()?.OpenReadAsync()?.join()?;
    let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
    let (width, height) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
    let scale = (WINDOWS_THUMBNAIL_EDGE as f32 / width.max(height).max(1) as f32).min(1.0);
    let (tw, th) = (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    );
    let transform = BitmapTransform::new()?;
    transform.SetScaledWidth(tw)?;
    transform.SetScaledHeight(th)?;
    transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    let pixels = decoder
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Rgba8,
            BitmapAlphaMode::Straight,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )?
        .join()?
        .DetachPixelData()?;
    Ok(WindowsContent::Image {
        width,
        height,
        thumbnail: ImagePayload {
            width: tw,
            height: th,
            rgba: pixels.to_vec(),
        },
    })
}
