//! Clipboard device-independent bitmaps: checked metadata, decode to a
//! normalized RGBA payload, and `CF_DIBV5` encode for paste-back.
//!
//! Windows-free and host-testable (see `docs/systemPatterns.md` §7.4).
//! Every size and offset is validated with checked arithmetic before any
//! pixel buffer is allocated; `BITMAPCOREHEADER` (12 bytes) is rejected.

use std::io::Cursor;

use image::DynamicImage;
use image::codecs::bmp::BmpDecoder;

/// A normalized clipboard image: top-down, straight (non-premultiplied)
/// 8-bit RGBA, `rgba.len() == width * height * 4`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImagePayload {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum DibError {
    /// Shorter than the declared or minimum header.
    Truncated,
    /// Header size other than 40, 52, 56, 108, or 124.
    HeaderSize(u32),
    /// Width not positive, height zero or `i32::MIN`.
    Dimensions,
    Planes(u16),
    /// Size arithmetic overflowed.
    Overflow,
    /// Decoded RGBA would exceed the per-item cap.
    TooLarge {
        bytes: u64,
    },
    /// Compression or bit depth outside the supported set.
    Unsupported,
    /// The decoder rejected data that passed the metadata checks.
    Decode,
}

impl std::fmt::Display for DibError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DibError::Truncated => write!(f, "truncated bitmap"),
            DibError::HeaderSize(n) => write!(f, "unsupported bitmap header size {n}"),
            DibError::Dimensions => write!(f, "invalid bitmap dimensions"),
            DibError::Planes(n) => write!(f, "invalid bitmap plane count {n}"),
            DibError::Overflow => write!(f, "bitmap size overflow"),
            DibError::TooLarge { bytes } => write!(f, "bitmap too large ({bytes} bytes)"),
            DibError::Unsupported => write!(f, "unsupported bitmap format"),
            DibError::Decode => write!(f, "bitmap data could not be decoded"),
        }
    }
}

impl std::error::Error for DibError {}

const BI_RGB: u32 = 0;
const BI_RLE8: u32 = 1;
const BI_RLE4: u32 = 2;
const BI_BITFIELDS: u32 = 3;

const V5_HEADER: usize = 124;
const LCS_SRGB: u32 = 0x7352_4742; // 'sRGB'
const LCS_GM_IMAGES: u32 = 4;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    u32_at(b, at) as i32
}

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Validate a `CF_DIB`/`CF_DIBV5` block and decode it. `item_cap` bounds the
/// decoded RGBA size (§7.5); the caller has already bounded the raw copy.
pub fn decode_dib(bytes: &[u8], item_cap: usize) -> Result<ImagePayload, DibError> {
    if bytes.len() < 40 {
        return Err(DibError::Truncated);
    }
    let header = u32_at(bytes, 0);
    if !matches!(header, 40 | 52 | 56 | 108 | 124) {
        return Err(DibError::HeaderSize(header));
    }
    if bytes.len() < header as usize {
        return Err(DibError::Truncated);
    }
    let width = i32_at(bytes, 4);
    let height = i32_at(bytes, 8);
    if width <= 0 || height == 0 || height == i32::MIN {
        return Err(DibError::Dimensions);
    }
    let planes = u16_at(bytes, 12);
    if planes != 1 {
        return Err(DibError::Planes(planes));
    }
    let bit_count = u16_at(bytes, 14) as u64;
    let compression = u32_at(bytes, 16);
    let size_image = u32_at(bytes, 20) as u64;
    let clr_used = u32_at(bytes, 32) as u64;

    let (w, h) = (width as u64, height.unsigned_abs() as u64);
    let rgba_bytes = w
        .checked_mul(h)
        .and_then(|px| px.checked_mul(4))
        .ok_or(DibError::Overflow)?;
    if rgba_bytes > item_cap as u64 {
        return Err(DibError::TooLarge { bytes: rgba_bytes });
    }

    let supported = match compression {
        BI_RGB => matches!(bit_count, 1 | 4 | 8 | 16 | 24 | 32),
        BI_BITFIELDS => matches!(bit_count, 16 | 32),
        BI_RLE8 => bit_count == 8,
        BI_RLE4 => bit_count == 4,
        _ => false,
    };
    if !supported {
        return Err(DibError::Unsupported);
    }

    // Layout: header, optional masks (only after a 40-byte header), palette, pixels.
    let masks = if compression == BI_BITFIELDS && header == 40 {
        12
    } else {
        0
    };
    // An indexed image that declares no palette size has one entry per index.
    let palette_entries = if bit_count <= 8 && clr_used == 0 {
        1 << bit_count
    } else {
        clr_used
    };
    if bit_count <= 8 && palette_entries > (1 << bit_count) {
        return Err(DibError::Unsupported);
    }
    let pixels = match compression {
        BI_RLE8 | BI_RLE4 => {
            if size_image == 0 {
                return Err(DibError::Unsupported);
            }
            size_image
        }
        _ => {
            let stride = w
                .checked_mul(bit_count)
                .and_then(|bits| bits.checked_add(31))
                .map(|bits| bits / 32 * 4)
                .ok_or(DibError::Overflow)?;
            stride.checked_mul(h).ok_or(DibError::Overflow)?
        }
    };
    let needed = (header as u64 + masks)
        .checked_add(palette_entries.checked_mul(4).ok_or(DibError::Overflow)?)
        .and_then(|n| n.checked_add(pixels))
        .ok_or(DibError::Overflow)?;
    if needed > bytes.len() as u64 {
        return Err(DibError::Truncated);
    }

    // `image` assumes a V4/V5 BI_BITFIELDS DIB repeats its RGB masks in 12
    // bytes after the header (as GDI-synthesized DIBs do). Producers that keep
    // the masks only inside the header are normalized to that layout.
    let patched;
    let bytes = if compression == BI_BITFIELDS
        && header >= 108
        && !has_trailing_masks(bytes, header as usize, needed)
    {
        let header = header as usize;
        let mut copy = Vec::with_capacity(bytes.len() + 12);
        copy.extend_from_slice(&bytes[..header]);
        copy.extend_from_slice(&bytes[40..52]);
        copy.extend_from_slice(&bytes[header..]);
        patched = copy;
        patched.as_slice()
    } else {
        bytes
    };

    let decoder =
        BmpDecoder::new_without_file_header(Cursor::new(bytes)).map_err(|_| DibError::Decode)?;
    let image = DynamicImage::from_decoder(decoder).map_err(|_| DibError::Decode)?;
    let rgba = image.to_rgba8();
    if (rgba.width() as u64, rgba.height() as u64) != (w, h) {
        return Err(DibError::Decode);
    }
    let payload = ImagePayload {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    };
    debug_assert_eq!(payload.rgba.len() as u64, rgba_bytes);
    Ok(payload)
}

/// Encode as a maximally compatible `CF_DIB`: `BITMAPINFOHEADER`, 24 bpp
/// `BI_RGB`, bottom-up rows padded to 4 bytes. Alpha is dropped (composited
/// over white), which is what alpha-unaware consumers would show anyway.
pub fn encode_dib24(image: &ImagePayload) -> Vec<u8> {
    let (w, h) = (image.width as usize, image.height as usize);
    let stride = (w * 3).div_ceil(4) * 4;
    let mut out = Vec::with_capacity(40 + stride * h);
    push_u32(&mut out, 40);
    push_u32(&mut out, image.width);
    push_u32(&mut out, image.height); // positive: bottom-up
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    push_u32(&mut out, BI_RGB);
    push_u32(&mut out, (stride * h) as u32);
    for _ in 0..4 {
        push_u32(&mut out, 0); // ppm x/y, colors used/important
    }
    let over_white = |c: u8, a: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32)) / 255) as u8;
    for y in (0..h).rev() {
        let row = &image.rgba[y * w * 4..(y + 1) * w * 4];
        for px in row.as_chunks::<4>().0 {
            let a = px[3];
            out.extend_from_slice(&[
                over_white(px[2], a),
                over_white(px[1], a),
                over_white(px[0], a),
            ]);
        }
        out.resize(out.len() + stride - w * 3, 0);
    }
    out
}

/// True when the 12 bytes after a V4/V5 header repeat its red/green/blue masks.
/// The length must leave room for them as well as the pixels (`needed`), or
/// pixels that happen to equal the masks would be mistaken for them.
fn has_trailing_masks(bytes: &[u8], header: usize, needed: u64) -> bool {
    bytes.len() as u64 >= needed + 12 && bytes[header..header + 12] == bytes[40..52]
}

/// Encode as `CF_DIBV5` (§7.4): a 124-byte `BITMAPV5HEADER` (top-down, 32 bpp,
/// `BI_BITFIELDS`, BGRA masks, sRGB) followed by BGRA pixels.
pub fn encode_dibv5(image: &ImagePayload) -> Vec<u8> {
    let pixel_bytes = image.rgba.len();
    let mut out = Vec::with_capacity(V5_HEADER + pixel_bytes);

    push_u32(&mut out, V5_HEADER as u32);
    push_u32(&mut out, image.width);
    push_u32(&mut out, (-(image.height as i32)) as u32); // top-down
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bit count
    push_u32(&mut out, BI_BITFIELDS);
    push_u32(&mut out, pixel_bytes as u32);
    push_u32(&mut out, 0); // x pixels per meter
    push_u32(&mut out, 0); // y pixels per meter
    push_u32(&mut out, 0); // colors used
    push_u32(&mut out, 0); // colors important
    push_u32(&mut out, 0x00FF_0000); // red mask
    push_u32(&mut out, 0x0000_FF00); // green mask
    push_u32(&mut out, 0x0000_00FF); // blue mask
    push_u32(&mut out, 0xFF00_0000); // alpha mask
    push_u32(&mut out, LCS_SRGB);
    out.extend_from_slice(&[0u8; 36]); // CIEXYZTRIPLE endpoints
    push_u32(&mut out, 0); // gamma red
    push_u32(&mut out, 0); // gamma green
    push_u32(&mut out, 0); // gamma blue
    push_u32(&mut out, LCS_GM_IMAGES);
    push_u32(&mut out, 0); // profile data
    push_u32(&mut out, 0); // profile size
    push_u32(&mut out, 0); // reserved
    debug_assert_eq!(out.len(), V5_HEADER);

    for px in image.rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAP: usize = 8 * 1024 * 1024;

    /// A BITMAPINFOHEADER DIB with `rows` given top-to-bottom as BGR(A) bytes.
    fn dib(width: i32, rows: &[&[u8]], bit_count: u16, top_down: bool) -> Vec<u8> {
        let height = rows.len() as i32;
        let mut out = Vec::new();
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&(if top_down { -height } else { height }).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&bit_count.to_le_bytes());
        out.extend_from_slice(&[0u8; 24]); // compression BI_RGB .. clr_important
        let stride = (width as usize * bit_count as usize).div_ceil(32) * 4;
        let ordered: Vec<&[u8]> = if top_down {
            rows.to_vec()
        } else {
            rows.iter().rev().copied().collect()
        };
        for row in ordered {
            let mut padded = row.to_vec();
            padded.resize(stride, 0);
            out.extend_from_slice(&padded);
        }
        out
    }

    #[test]
    fn decodes_bottom_up_24_bit() {
        // 2x2: top row red, green; bottom row blue, white (BGR in file).
        let bytes = dib(
            2,
            &[&[0, 0, 255, 0, 255, 0], &[255, 0, 0, 255, 255, 255]],
            24,
            false,
        );
        let image = decode_dib(&bytes, CAP).unwrap();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(
            image.rgba,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, //
                0, 0, 255, 255, 255, 255, 255, 255,
            ]
        );
    }

    #[test]
    fn decodes_top_down_32_bit() {
        let bytes = dib(1, &[&[10, 20, 30, 0], &[40, 50, 60, 0]], 32, true);
        let image = decode_dib(&bytes, CAP).unwrap();
        assert_eq!(image.rgba.len(), 2 * 4);
        assert_eq!(&image.rgba[..3], &[30, 20, 10]);
        assert_eq!(&image.rgba[4..7], &[60, 50, 40]);
    }

    #[test]
    fn rejects_truncated_header_and_data() {
        let bytes = dib(2, &[&[0; 6], &[0; 6]], 24, false);
        assert_eq!(decode_dib(&bytes[..39], CAP), Err(DibError::Truncated));
        assert_eq!(
            decode_dib(&bytes[..bytes.len() - 1], CAP),
            Err(DibError::Truncated)
        );
        let mut v5_claim = bytes.clone();
        v5_claim[0..4].copy_from_slice(&124u32.to_le_bytes());
        v5_claim.truncate(100);
        assert_eq!(decode_dib(&v5_claim, CAP), Err(DibError::Truncated));
    }

    #[test]
    fn rejects_bad_header_planes_and_dimensions() {
        let good = dib(2, &[&[0; 6], &[0; 6]], 24, false);
        let mut core = good.clone();
        core[0..4].copy_from_slice(&12u32.to_le_bytes());
        assert_eq!(decode_dib(&core, CAP), Err(DibError::HeaderSize(12)));

        let mut planes = good.clone();
        planes[12..14].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(decode_dib(&planes, CAP), Err(DibError::Planes(2)));

        for (w, h) in [(0i32, 2), (-1, 2), (2, 0), (2, i32::MIN)] {
            let mut bad = good.clone();
            bad[4..8].copy_from_slice(&w.to_le_bytes());
            bad[8..12].copy_from_slice(&h.to_le_bytes());
            assert_eq!(decode_dib(&bad, CAP), Err(DibError::Dimensions), "{w}x{h}");
        }
    }

    #[test]
    fn rejects_overflow_and_decoded_size_cap() {
        let good = dib(2, &[&[0; 6], &[0; 6]], 24, false);
        let mut huge = good.clone();
        huge[4..8].copy_from_slice(&i32::MAX.to_le_bytes());
        huge[8..12].copy_from_slice(&i32::MAX.to_le_bytes());
        // i32::MAX^2 * 4 fits in u64, so the cap reports it; nothing is allocated.
        assert!(matches!(
            decode_dib(&huge, usize::MAX),
            Err(DibError::Truncated | DibError::Overflow)
        ));
        assert!(matches!(
            decode_dib(&huge, CAP),
            Err(DibError::TooLarge { .. })
        ));
        // Exactly at the cap is accepted; one byte under is rejected.
        assert!(decode_dib(&good, 16).is_ok());
        assert_eq!(decode_dib(&good, 15), Err(DibError::TooLarge { bytes: 16 }));
    }

    #[test]
    fn rejects_unsupported_compression() {
        let mut jpeg = dib(1, &[&[0; 3]], 24, false);
        jpeg[16..20].copy_from_slice(&4u32.to_le_bytes()); // BI_JPEG
        assert_eq!(decode_dib(&jpeg, CAP), Err(DibError::Unsupported));
    }

    #[test]
    fn dibv5_header_and_pixel_order() {
        let image = ImagePayload {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };
        let out = encode_dibv5(&image);
        assert_eq!(out.len(), 124 + 8);
        assert_eq!(u32_at(&out, 0), 124);
        assert_eq!(i32_at(&out, 4), 2);
        assert_eq!(i32_at(&out, 8), -1);
        assert_eq!(u16_at(&out, 12), 1);
        assert_eq!(u16_at(&out, 14), 32);
        assert_eq!(u32_at(&out, 16), BI_BITFIELDS);
        assert_eq!(u32_at(&out, 20), 8);
        assert_eq!(
            [
                u32_at(&out, 40),
                u32_at(&out, 44),
                u32_at(&out, 48),
                u32_at(&out, 52)
            ],
            [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000]
        );
        assert_eq!(u32_at(&out, 56), LCS_SRGB);
        assert_eq!(&out[124..], &[3, 2, 1, 4, 7, 6, 5, 8]);

        // And it decodes back to the same straight-alpha RGBA.
        assert_eq!(decode_dib(&out, CAP).unwrap(), image);
    }

    #[test]
    fn dib24_is_bottom_up_padded_and_round_trips_opaque_pixels() {
        let image = ImagePayload {
            width: 1,
            height: 2,
            rgba: vec![10, 20, 30, 255, 40, 50, 60, 255],
        };
        let out = encode_dib24(&image);
        assert_eq!(out.len(), 40 + 4 * 2); // 3-byte rows padded to 4
        assert_eq!(i32_at(&out, 8), 2);
        assert_eq!(u16_at(&out, 14), 24);
        assert_eq!(u32_at(&out, 16), BI_RGB);
        // Bottom row first, BGR.
        assert_eq!(&out[40..44], &[60, 50, 40, 0]);
        assert_eq!(&out[44..48], &[30, 20, 10, 0]);
        assert_eq!(decode_dib(&out, CAP).unwrap(), image);
        // Transparent pixels composite over white.
        let clear = ImagePayload {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 0],
        };
        assert_eq!(&encode_dib24(&clear)[40..43], &[255, 255, 255]);
    }

    #[test]
    fn dibv5_with_trailing_masks_decodes_identically() {
        // GDI-synthesized CF_DIBV5 repeats the RGB masks after the header.
        let image = ImagePayload {
            width: 1,
            height: 2,
            rgba: vec![10, 20, 30, 128, 40, 50, 60, 255],
        };
        let plain = encode_dibv5(&image);
        let mut trailing = plain[..124].to_vec();
        trailing.extend_from_slice(&plain[40..52]);
        trailing.extend_from_slice(&plain[124..]);
        assert_eq!(decode_dib(&trailing, CAP).unwrap(), image);
        assert_eq!(decode_dib(&plain, CAP).unwrap(), image);
    }

    #[test]
    fn dibv5_pixels_equal_to_the_masks_are_not_masks() {
        // Transparent pure red, green, blue: BGRA bytes match the masks.
        let image = ImagePayload {
            width: 3,
            height: 1,
            rgba: vec![255, 0, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0],
        };
        let encoded = encode_dibv5(&image);
        assert_eq!(encoded[124..136], encoded[40..52]);
        assert_eq!(decode_dib(&encoded, CAP).unwrap(), image);
    }
}
