//! Palette placement on summon (plan §12.7): move the hidden palette onto the
//! monitor under the cursor, centered in its work area and raised above center,
//! before it becomes visible.
//!
//! Positioning uses physical pixels through `SetWindowPos` rather than egui's
//! `OuterPosition`, which converts points with the scale factor of the monitor
//! the window is leaving. Crossing to a monitor with a different DPI makes winit
//! rescale the window (`WM_DPICHANGED`), so the move is re-centered with the
//! size the window actually ended up with.
//!
//! Fast summon: eframe delays repaints of a hidden window by at least 100 ms,
//! so the hotkey and tray handlers (main thread) show the window themselves
//! with [`PaletteWindow::begin_reveal`]: placed, shown without activation, and
//! DWM-cloaked so nothing stale is composed. The repaint they request then runs
//! at once; the host uncloaks after the first fresh frame is presented.

use std::sync::atomic::{AtomicBool, Ordering};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowRect, IsWindowVisible, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, ShowWindow,
};
use windows::core::BOOL;

use crate::core::placement::{Rect, palette_origin, rescale};

/// The palette is shown cloaked by a handler and not yet uncloaked. There is
/// one palette window per process.
static REVEALING: AtomicBool = AtomicBool::new(false);

/// The palette's top-level window. Stored as an integer so handlers on any
/// thread can hold it; Win32 window calls are valid from any thread.
#[derive(Clone, Copy, Debug)]
pub struct PaletteWindow {
    hwnd: isize,
}

impl PaletteWindow {
    /// Resolve the Win32 window from eframe's creation context. `None` when the
    /// handle is unavailable; placement is then left to the system.
    pub fn from_handle(source: &impl HasWindowHandle) -> Option<Self> {
        match source.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(Self {
                hwnd: handle.hwnd.get(),
            }),
            _ => None,
        }
    }

    pub(crate) fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    /// The handle as an integer, for a thread that owns a dialog.
    pub(crate) fn raw(&self) -> isize {
        self.hwnd
    }

    /// Summon fast path, called by a handler before it sends the summon event.
    /// A hidden palette is placed and shown cloaked without activation, so the
    /// paste destination is still the foreground window when the model
    /// captures it. Does nothing when the palette is already visible.
    pub fn begin_reveal(&self) {
        if unsafe { IsWindowVisible(self.hwnd()) }.as_bool() {
            return;
        }
        // Without a cloak the stale surface would show; keep the normal path.
        if !self.set_cloak(true) {
            return;
        }
        REVEALING.store(true, Ordering::Release);
        self.place_on_cursor_monitor();
        let _ = unsafe { ShowWindow(self.hwnd(), SW_SHOWNOACTIVATE) };
    }

    /// Whether a handler showed the window cloaked and it is not yet revealed.
    pub fn revealing(&self) -> bool {
        REVEALING.load(Ordering::Acquire)
    }

    /// Uncloak once a fresh frame has been presented.
    pub fn finish_reveal(&self) {
        if REVEALING.swap(false, Ordering::AcqRel) {
            self.set_cloak(false);
        }
    }

    /// The model did not show the palette after all: hide it again directly
    /// (winit never saw it shown), then uncloak the hidden window.
    pub fn cancel_reveal(&self) {
        if REVEALING.swap(false, Ordering::AcqRel) {
            let _ = unsafe { ShowWindow(self.hwnd(), SW_HIDE) };
            self.set_cloak(false);
        }
    }

    fn set_cloak(&self, cloaked: bool) -> bool {
        let value = BOOL::from(cloaked);
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd(),
                DWMWA_CLOAK,
                (&value as *const BOOL).cast(),
                size_of::<BOOL>() as u32,
            )
        }
        .is_ok()
    }

    /// Move the window to the cursor's monitor. Best effort: any failed query
    /// leaves the window where it is.
    pub fn place_on_cursor_monitor(&self) {
        let _ = self.try_place();
    }

    fn try_place(&self) -> Option<()> {
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) }.ok()?;
        let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe { GetMonitorInfoW(monitor, &mut info) }.ok().ok()?;
        let work = to_rect(info.rcWork);

        let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.ok()?;
        let window_dpi = unsafe { GetDpiForWindow(self.hwnd()) };

        // Predict the size at the target DPI so the first move is already
        // centered; a DPI change then only confirms it.
        let current = self.window_rect()?;
        let width = rescale(current.width(), window_dpi, dpi_x);
        let height = rescale(current.height(), window_dpi, dpi_x);
        self.move_to(palette_origin(work, width, height))?;

        let placed = self.window_rect()?;
        if placed.width() != width || placed.height() != height {
            self.move_to(palette_origin(work, placed.width(), placed.height()))?;
        }
        Some(())
    }

    fn window_rect(&self) -> Option<Rect> {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(self.hwnd(), &mut rect) }.ok()?;
        Some(to_rect(rect))
    }

    fn move_to(&self, (x, y): (i32, i32)) -> Option<()> {
        unsafe {
            SetWindowPos(
                self.hwnd(),
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .ok()
    }
}

fn to_rect(rect: RECT) -> Rect {
    Rect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
