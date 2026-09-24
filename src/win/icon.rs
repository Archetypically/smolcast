//! Shell image → owned RGBA (plan §6.4).
//!
//! The shell hands icons out as a GDI `HBITMAP` (`IShellItemImageFactory`),
//! which must not cross threads. It is converted to an owned top-down RGBA
//! buffer on the thread that received it, and the bitmap is deleted there.

use core::ffi::c_void;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HBITMAP, HGDIOBJ, ReleaseDC,
};

/// An owned icon image: top-down, 8-bit RGBA with premultiplied alpha, which
/// is what the shell's 32-bit icon bitmaps carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconRgba {
    pub width: u32,
    pub height: u32,
    pub rgba_premultiplied: Vec<u8>,
}

/// Owns an `HBITMAP` and deletes it on drop.
pub struct OwnedBitmap(pub HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
    }
}

/// Largest edge accepted from the shell; guards the pixel allocation.
const MAX_EDGE: u32 = 256;

/// Copy `bitmap` into an owned RGBA buffer. Returns `None` for an empty or
/// oversized bitmap, or when GDI refuses the copy.
pub fn bitmap_to_rgba(bitmap: &OwnedBitmap) -> Option<IconRgba> {
    let mut info = BITMAP::default();
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0.0),
            size_of::<BITMAP>() as i32,
            Some(&mut info as *mut BITMAP as *mut c_void),
        )
    };
    if got == 0 {
        return None;
    }
    let width = u32::try_from(info.bmWidth).ok()?;
    let height = info.bmHeight.unsigned_abs();
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return None;
    }

    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            // Negative height requests top-down rows.
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    let dc = unsafe { GetDC(None) };
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap.0,
            0,
            height,
            Some(bgra.as_mut_ptr().cast()),
            &mut header,
            DIB_RGB_COLORS,
        )
    };
    unsafe { ReleaseDC(None, dc) };
    if lines != height as i32 {
        return None;
    }

    // A bitmap without an alpha channel reads back as all-zero alpha; treat it
    // as opaque rather than invisible.
    let has_alpha = bgra.as_chunks::<4>().0.iter().any(|px| px[3] != 0);
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        if !has_alpha {
            px[3] = 255;
        }
    }
    Some(IconRgba {
        width,
        height,
        rgba_premultiplied: bgra,
    })
}
