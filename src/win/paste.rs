//! Paste service (plan §8): destination identity, foreground handoff, and the
//! bounded paste finisher.
//!
//! The UI thread captures the destination (§8.1) and, after the clipboard
//! write receipt, validates it and hands foreground back (§8.2 steps 3–5).
//! Once the palette hides, a short-lived finisher thread waits for the
//! destination to become foreground and for every physical modifier to be
//! released, then injects one Ctrl+V (steps 6–8). A thread is used because
//! eframe throttles an invisible window's repaints to 100 ms, which is too
//! coarse for these deadlines (plan §8.2 timing revision).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
};

use crate::app::model::{AppEvent, Wake};

/// A captured paste destination (§8.1.3), belonging to one summon session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Destination {
    pub hwnd: HWND,
    pub pid: u32,
    pub tid: u32,
}

/// Capture the foreground window immediately before showing the palette (§8.1).
///
/// Rejects a null foreground window, an unknown window, and any window owned by
/// the smolcast process. The palette itself belongs to this process, so the PID
/// check inherently excludes it.
pub fn capture_destination() -> Option<Destination> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let (pid, tid) = window_identity(hwnd)?;
    if pid == unsafe { GetCurrentProcessId() } || is_shell_surface(hwnd) {
        return None;
    }
    Some(Destination { hwnd, pid, tid })
}

/// The taskbar and desktop take focus when the tray icon is clicked or the
/// desktop is selected; neither accepts a paste, so they are no destination
/// and paste-back falls back to "copied, not pasted" (§8.1).
fn is_shell_surface(hwnd: HWND) -> bool {
    let mut class = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut class) };
    let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
    matches!(
        class.as_str(),
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW"
    )
}

fn window_identity(hwnd: HWND) -> Option<(u32, u32)> {
    let mut pid = 0u32;
    let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (tid != 0 && pid != 0).then_some((pid, tid))
}

/// The window still exists with the same process and thread (§8.2 step 3).
pub fn destination_intact(dest: &Destination) -> bool {
    unsafe { IsWindow(Some(dest.hwnd)) }.as_bool()
        && window_identity(dest.hwnd) == Some((dest.pid, dest.tid))
}

/// The foreground window belongs to smolcast, i.e. the palette (§8.2 step 4).
pub fn palette_is_foreground() -> bool {
    let fg = unsafe { GetForegroundWindow() };
    !fg.is_invalid()
        && window_identity(fg).is_some_and(|(pid, _)| pid == unsafe { GetCurrentProcessId() })
}

/// Hand foreground to the destination while smolcast still owns it (§8.2
/// step 4). False means the system refused.
pub fn restore_foreground(dest: &Destination) -> bool {
    unsafe { SetForegroundWindow(dest.hwnd) }.as_bool()
}

/// How a paste attempt ended. Content-free.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteOutcome {
    Pasted,
    /// A newer summon or shutdown cancelled the attempt.
    Cancelled,
    /// The destination closed or its identity changed.
    DestinationChanged,
    /// Another window became foreground.
    FocusMoved,
    /// The destination did not become foreground within 150 ms.
    ForegroundTimeout,
    /// A modifier stayed held for 750 ms.
    ModifiersHeld,
    /// `SendInput` inserted fewer than four events.
    InjectionBlocked,
}

const FOREGROUND_DEADLINE: Duration = Duration::from_millis(150);
const MODIFIER_DEADLINE: Duration = Duration::from_millis(750);
/// Pause after the destination is foreground so apps that restore keyboard
/// focus asynchronously (Chromium/Electron, XAML islands) have focused their
/// input before Ctrl+V arrives. Identity and foreground are still checked.
const FOCUS_SETTLE: Duration = Duration::from_millis(75);
const TICK: Duration = Duration::from_millis(10);

const MODIFIERS: [VIRTUAL_KEY; 8] = [
    VK_LCONTROL,
    VK_RCONTROL,
    VK_LSHIFT,
    VK_RSHIFT,
    VK_LMENU,
    VK_RMENU,
    VK_LWIN,
    VK_RWIN,
];

/// Run §8.2 steps 6–8 on a bounded thread. The result arrives as
/// `AppEvent::PasteFinished`. Setting `cancel` stops it at the next tick.
pub fn spawn_finisher(
    dest: Destination,
    cancel: Arc<AtomicBool>,
    wake: Wake,
) -> std::io::Result<JoinHandle<()>> {
    // HWND is a raw pointer and not Send; move the integer value.
    let (raw, pid, tid) = (dest.hwnd.0 as isize, dest.pid, dest.tid);
    thread::Builder::new()
        .name("smolcast-paste".into())
        .spawn(move || {
            let dest = Destination {
                hwnd: HWND(raw as *mut core::ffi::c_void),
                pid,
                tid,
            };
            let outcome = finish(&dest, &cancel);
            wake.send(AppEvent::PasteFinished(outcome));
        })
}

fn finish(dest: &Destination, cancel: &AtomicBool) -> PasteOutcome {
    let ours = unsafe { GetCurrentProcessId() };

    // Step 6: the destination becomes foreground within 150 ms.
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return PasteOutcome::Cancelled;
        }
        if !destination_intact(dest) {
            return PasteOutcome::DestinationChanged;
        }
        let fg = unsafe { GetForegroundWindow() };
        if fg == dest.hwnd {
            break;
        }
        // Null and our own (hiding) palette are transitional; anything else is
        // the user choosing another window.
        if !fg.is_invalid() && window_identity(fg).is_some_and(|(pid, _)| pid != ours) {
            return PasteOutcome::FocusMoved;
        }
        if start.elapsed() > FOREGROUND_DEADLINE {
            return PasteOutcome::ForegroundTimeout;
        }
        thread::sleep(TICK);
    }

    // Step 7: every physical modifier released within 750 ms; nothing is
    // released on the user's behalf. The focus settle runs concurrently.
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return PasteOutcome::Cancelled;
        }
        if !destination_intact(dest) {
            return PasteOutcome::DestinationChanged;
        }
        if unsafe { GetForegroundWindow() } != dest.hwnd {
            return PasteOutcome::FocusMoved;
        }
        let held = MODIFIERS
            .iter()
            .any(|vk| unsafe { GetAsyncKeyState(i32::from(vk.0)) } as u16 & 0x8000 != 0);
        if !held && start.elapsed() >= FOCUS_SETTLE {
            break;
        }
        if start.elapsed() > MODIFIER_DEADLINE {
            return PasteOutcome::ModifiersHeld;
        }
        thread::sleep(TICK);
    }

    // Step 8: one array, no retry.
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let v = VIRTUAL_KEY(u16::from(b'V'));
    let inputs = [
        key(VK_CONTROL, false),
        key(v, false),
        key(v, true),
        key(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        PasteOutcome::Pasted
    } else {
        PasteOutcome::InjectionBlocked
    }
}
