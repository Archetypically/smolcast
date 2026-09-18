//! Paste service: destination identity capture (plan §8.1). The bounded paste
//! state machine (clipboard write, foreground revalidation, modifier wait, and
//! `SendInput`) arrives with milestone 3; this module ships only the capture half
//! that milestone 1 requires.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

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
    let mut pid = 0u32;
    let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if tid == 0 || pid == 0 || pid == unsafe { GetCurrentProcessId() } {
        return None;
    }
    Some(Destination { hwnd, pid, tid })
}
