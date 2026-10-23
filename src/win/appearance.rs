//! Window appearance (plan §12.7): Windows 11 rounded corners, a dark/light
//! frame that follows the egui theme, conditional Mica with a solid fallback,
//! and the system UI font.
//!
//! Mica needs Windows 11 22H2 (build 22621) for `DWMWA_SYSTEMBACKDROP_TYPE`,
//! "Transparency effects" on, and high contrast off. It is decided once at
//! startup because the GL surface must be created transparent; if the user
//! turns transparency off later, DWM draws its own solid fallback. Everything
//! here is best effort: a failed call leaves the default opaque window.

use std::path::PathBuf;

use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::Graphics::Dwm::{
    DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_ROUTINE_FLAGS, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ, RegGetValueW,
};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW,
};
use windows::core::{BOOL, PCWSTR, w};

use crate::win::from_wide_nul;
use crate::win::placement::PaletteWindow;

/// First build with `DWMWA_SYSTEMBACKDROP_TYPE` (Windows 11 22H2).
const MICA_MIN_BUILD: u32 = 22621;

/// Whether to create the palette transparent and request Mica.
pub fn mica_available() -> bool {
    windows_build().is_some_and(|build| build >= MICA_MIN_BUILD)
        && transparency_enabled()
        && !high_contrast()
}

/// One-time frame styling: rounded corners, plus the Mica backdrop when the
/// window was created transparent for it.
pub fn style_window(window: &PaletteWindow, mica: bool) {
    let hwnd = window.hwnd();
    set_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND);
    if mica {
        // The backdrop shows through the transparent GL surface. The frame is
        // deliberately not extended into the client area: DWM would then draw
        // minimize/maximize/close buttons over the palette.
        set_attribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &DWMSBT_MAINWINDOW);
    }
}

/// Dark frame and Mica tint for a dark egui theme, light otherwise.
pub fn set_dark(window: &PaletteWindow, dark: bool) {
    set_attribute(
        window.hwnd(),
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &BOOL::from(dark),
    );
}

/// The system UI font file (Segoe UI), if present.
pub fn system_font() -> Option<Vec<u8>> {
    read_font("segoeui.ttf")
}

/// Windows' own icon font, which the Start menu and Settings draw their
/// icons with: Segoe Fluent Icons on Windows 11, Segoe MDL2 Assets on
/// Windows 10. They share codepoints.
pub fn icon_font() -> Option<Vec<u8>> {
    read_font("SegoeIcons.ttf").or_else(|| read_font("segmdl2.ttf"))
}

fn read_font(file: &str) -> Option<Vec<u8>> {
    let dir = std::env::var_os("WINDIR")?;
    std::fs::read(PathBuf::from(dir).join("Fonts").join(file)).ok()
}

fn set_attribute<T>(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: &T) {
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (value as *const T).cast(),
            size_of::<T>() as u32,
        )
    };
}

fn windows_build() -> Option<u32> {
    let mut buffer = [0u16; 16];
    let mut bytes = size_of_val(&buffer) as u32;
    read_value(
        HKEY_LOCAL_MACHINE,
        w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
        w!("CurrentBuildNumber"),
        RRF_RT_REG_SZ,
        buffer.as_mut_ptr().cast(),
        &mut bytes,
    )?;
    from_wide_nul(&buffer).parse().ok()
}

/// Settings > Personalization > Colors > Transparency effects. Absent means on.
fn transparency_enabled() -> bool {
    let mut value = 1u32;
    let mut bytes = size_of::<u32>() as u32;
    let _ = read_value(
        HKEY_CURRENT_USER,
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
        w!("EnableTransparency"),
        RRF_RT_REG_DWORD,
        (&mut value as *mut u32).cast(),
        &mut bytes,
    );
    value != 0
}

/// Settings > Accessibility > Visual effects > Animation effects. Assumed on
/// when the query fails.
pub fn animations_enabled() -> bool {
    let mut enabled = BOOL::from(true);
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    enabled.as_bool()
}

/// Whether a Windows high-contrast theme is on.
pub fn high_contrast() -> bool {
    let mut info = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.cbSize,
            Some((&mut info as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok()
        && info.dwFlags.contains(HCF_HIGHCONTRASTON)
}

fn read_value(
    root: HKEY,
    subkey: PCWSTR,
    value: PCWSTR,
    flags: REG_ROUTINE_FLAGS,
    data: *mut core::ffi::c_void,
    bytes: &mut u32,
) -> Option<()> {
    let status = unsafe { RegGetValueW(root, subkey, value, flags, None, Some(data), Some(bytes)) };
    (status == ERROR_SUCCESS).then_some(())
}
