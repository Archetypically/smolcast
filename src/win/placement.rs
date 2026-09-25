//! Palette placement on summon (plan §12.7): move the hidden palette onto the
//! monitor under the cursor, centered in its work area and raised above center,
//! before it becomes visible.
//!
//! Positioning uses physical pixels through `SetWindowPos` rather than egui's
//! `OuterPosition`, which converts points with the scale factor of the monitor
//! the window is leaving. Crossing to a monitor with a different DPI makes winit
//! rescale the window (`WM_DPICHANGED`), so the move is re-centered with the
//! size the window actually ended up with.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowRect, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};

use crate::core::placement::{Rect, palette_origin, rescale};

/// The palette's top-level window.
#[derive(Clone, Copy, Debug)]
pub struct PaletteWindow {
    hwnd: HWND,
}

impl PaletteWindow {
    /// Resolve the Win32 window from eframe's creation context. `None` when the
    /// handle is unavailable; placement is then left to the system.
    pub fn from_handle(source: &impl HasWindowHandle) -> Option<Self> {
        match source.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(Self {
                hwnd: HWND(handle.hwnd.get() as *mut _),
            }),
            _ => None,
        }
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
        let window_dpi = unsafe { GetDpiForWindow(self.hwnd) };

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
        unsafe { GetWindowRect(self.hwnd, &mut rect) }.ok()?;
        Some(to_rect(rect))
    }

    fn move_to(&self, (x, y): (i32, i32)) -> Option<()> {
        unsafe {
            SetWindowPos(
                self.hwnd,
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
