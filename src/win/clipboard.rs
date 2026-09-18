//! Clipboard worker thread (plan §1.3, §7.1).
//!
//! One dedicated thread owns one message-only HWND and, from milestone 3
//! onward, every clipboard read/write. Milestone 1 ships the thread, its class
//! registration, the command wake (`WM_APP + 1`), the ready handshake, and the
//! ordered shutdown path. `AddClipboardFormatListener`, capture, filtering, and
//! normalization arrive together with milestone 3.
//!
//! The UI wakes the thread by sending a command and posting `WAKE_COMMAND`.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HWND_MESSAGE,
    MSG, PostMessageW, PostQuitMessage, RegisterClassW, TranslateMessage, UnregisterClassW,
    WINDOW_EX_STYLE, WM_APP, WM_DESTROY, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::PCWSTR;

/// Commands sent to the clipboard thread. Milestone 1 ships only `Shutdown`;
/// `SetText`, `SetImage`, and `SetCaptureEnabled` are added with capture in
/// milestone 3 (§7.1).
#[derive(Debug)]
pub enum ClipboardCommand {
    Shutdown,
}

/// `WM_APP + 1` wakes the message loop so it drains queued commands (§1.3).
const WAKE_COMMAND: u32 = WM_APP + 1;

#[derive(Debug)]
pub struct ClipboardError {
    message: String,
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ClipboardError {}

/// Handle to the clipboard worker. `shutdown` performs the ordered teardown
/// (§1.3): remove the listener (milestone 3), destroy the HWND, unregister the
/// class, post quit, and join.
pub struct ClipboardThread {
    command_tx: Sender<ClipboardCommand>,
    hwnd: HWND,
    join: Option<JoinHandle<()>>,
}

/// The ready handshake crosses a thread boundary, and `HWND` (a raw pointer) is
/// not `Send`; the integer value is moved instead and rebuilt on this side.
type ReadyValue = Result<isize, String>;

impl ClipboardThread {
    /// Spawn the thread and block on the ready handshake. Ready is sent only
    /// after the class is registered and the HWND created, so a startup failure
    /// is visible before eframe begins (§1.3).
    pub fn start() -> Result<Self, ClipboardError> {
        let (command_tx, command_rx) = mpsc::channel::<ClipboardCommand>();
        let (ready_tx, ready_rx) = mpsc::channel::<ReadyValue>();
        let join = thread::Builder::new()
            .name("smolcast-clipboard".into())
            .spawn(move || clipboard_main(command_rx, ready_tx))
            .map_err(|e| ClipboardError {
                message: format!("spawn clipboard thread: {e}"),
            })?;

        let raw = ready_rx
            .recv()
            .map_err(|e| ClipboardError {
                message: format!("clipboard thread died before ready: {e}"),
            })?
            .map_err(|message| ClipboardError { message })?;
        let hwnd = HWND(raw as *mut core::ffi::c_void);

        Ok(Self {
            command_tx,
            hwnd,
            join: Some(join),
        })
    }

    /// Wake the message loop so it drains commands (§1.3). Future milestones
    /// send `SetText`/`SetImage`/`SetCaptureEnabled` through this path.
    pub fn send_command(&self, command: ClipboardCommand) {
        if self.command_tx.send(command).is_ok() {
            let _ = unsafe { PostMessageW(Some(self.hwnd), WAKE_COMMAND, WPARAM(0), LPARAM(0)) };
        }
    }

    /// Ordered shutdown: enqueue `Shutdown`, wake, join (§1.5 step 4).
    pub fn shutdown(mut self) {
        self.stop_and_join();
    }
}

impl Drop for ClipboardThread {
    /// Safety net: on abnormal teardown (for example an app panic that skips
    /// `on_exit`), still stop the thread and destroy its message-only window.
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

impl ClipboardThread {
    fn stop_and_join(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = self.command_tx.send(ClipboardCommand::Shutdown);
            let _ = unsafe { PostMessageW(Some(self.hwnd), WAKE_COMMAND, WPARAM(0), LPARAM(0)) };
            let _ = join.join();
        }
    }
}

fn clipboard_main(command_rx: Receiver<ClipboardCommand>, ready: Sender<ReadyValue>) {
    let class_name = format!("smolcast-clip-{}", unsafe { GetCurrentProcessId() });
    let class_wide: Vec<u16> = class_name.encode_utf16().chain([0]).collect();

    let hinstance = match unsafe { GetModuleHandleW(None) } {
        Ok(module) => HINSTANCE(module.0),
        Err(e) => {
            let _ = ready.send(Err(format!("GetModuleHandleW: {e}")));
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
        let _ = ready.send(Err(format!(
            "RegisterClassW failed: {}",
            std::io::Error::last_os_error()
        )));
        return;
    }

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
                Ok(_) => "CreateWindowExW returned an invalid HWND".into(),
                Err(e) => format!("CreateWindowExW: {e}"),
            }));
            let _ = unsafe { UnregisterClassW(PCWSTR(class_wide.as_ptr()), Some(hinstance)) };
            return;
        }
    };

    if ready.send(Ok(hwnd.0 as isize)).is_err() {
        // The UI side died before the handshake; run a minimal teardown.
        let _ = unsafe { DestroyWindow(hwnd) };
        let _ = unsafe { UnregisterClassW(PCWSTR(class_wide.as_ptr()), Some(hinstance)) };
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
        };
        unsafe { DispatchMessageW(&msg) };
        if drain_commands(&command_rx, hwnd) {
            break; // Shutdown handled; WM_QUIT may already be queued.
        }
    }

    let _ = unsafe { DestroyWindow(hwnd) };
    let _ = unsafe { UnregisterClassW(PCWSTR(class_wide.as_ptr()), Some(hinstance)) };
}

/// Returns true when a `Shutdown` command was processed.
fn drain_commands(command_rx: &Receiver<ClipboardCommand>, hwnd: HWND) -> bool {
    let mut stop = false;
    for cmd in command_rx.try_iter() {
        match cmd {
            ClipboardCommand::Shutdown => {
                // Sends WM_DESTROY synchronously; the proc posts WM_QUIT.
                stop = true;
            }
        }
    }
    if stop {
        let _ = unsafe { DestroyWindow(hwnd) };
    }
    stop
}

unsafe extern "system" fn clipboard_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WAKE_COMMAND => {
            // The message loop drains its channel after dispatch; nothing to do
            // here beyond acknowledging the wake.
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
