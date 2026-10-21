//! Icon downscaling: area-average resampling of premultiplied RGBA.
//!
//! Windows-free and host-testable. The shell resizes icons to odd edges
//! (22 px at 100%, 33 px at 150%) with a coarse filter, so app icons are
//! fetched at a larger stock size and reduced here instead. Averaging
//! premultiplied pixels keeps transparent edges free of dark fringes.

/// Smallest fetch edge the shell is asked for: at least twice the drawn edge,
/// so the reduction always averages several source pixels per output pixel.
pub fn fetch_edge(target: u32) -> u32 {
    const STOCK: [u32; 5] = [48, 64, 96, 128, 256];
    let want = target.saturating_mul(2);
    STOCK.into_iter().find(|&s| s >= want).unwrap_or(256)
}

/// Crop the empty margin the shell adds when it serves an icon larger than
/// any frame the icon has: a 48 px-only icon (7-Zip) comes back centered in a
/// mostly transparent 96 px canvas and would draw at half size. Only margins
/// that leave the art at 60% of the edge or less are cropped; designed icon
/// padding is far smaller. Returns the input unchanged otherwise.
pub fn trim_padding(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let (w, h) = (width as usize, height as usize);
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for (i, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
        if px[3] != 0 {
            let (x, y) = (i % w, i / w);
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
    }
    let Some((x0, y0, x1, y1)) = bounds else {
        return (width, height, rgba.to_vec());
    };
    let side = (x1 - x0 + 1).max(y1 - y0 + 1);
    // A square that cannot fit a non-square image is left alone.
    if side * 10 > w.max(h) * 6 || side > w.min(h) {
        return (width, height, rgba.to_vec());
    }
    // A square around the art's center, kept inside the image.
    let cx = (x0 + x1).div_ceil(2);
    let cy = (y0 + y1).div_ceil(2);
    let left = cx.saturating_sub(side / 2).min(w - side);
    let top = cy.saturating_sub(side / 2).min(h - side);
    let mut out = Vec::with_capacity(side * side * 4);
    for y in top..top + side {
        out.extend_from_slice(&rgba[(y * w + left) * 4..][..side * 4]);
    }
    (side as u32, side as u32, out)
}

/// Reduce a `width`×`height` premultiplied RGBA image so its longer side is
/// `edge`, preserving aspect ratio. Images already within `edge` are
/// returned unchanged. Returns `(width, height, pixels)`.
pub fn downscale_premultiplied(
    width: u32,
    height: u32,
    rgba: &[u8],
    edge: u32,
) -> (u32, u32, Vec<u8>) {
    debug_assert_eq!(rgba.len(), (width * height * 4) as usize);
    let longer = width.max(height);
    if edge == 0 || longer <= edge {
        return (width, height, rgba.to_vec());
    }
    let out_w = ((u64::from(width) * u64::from(edge)).div_ceil(u64::from(longer)) as u32).max(1);
    let out_h = ((u64::from(height) * u64::from(edge)).div_ceil(u64::from(longer)) as u32).max(1);

    // Separable box filter: rows first, then columns.
    let x_weights = coverage(width, out_w);
    let y_weights = coverage(height, out_h);
    let mut rows = vec![0f32; (out_w * height * 4) as usize];
    for y in 0..height as usize {
        let src = &rgba[y * width as usize * 4..][..width as usize * 4];
        let dst = &mut rows[y * out_w as usize * 4..][..out_w as usize * 4];
        for (ox, taps) in x_weights.iter().enumerate() {
            for &(sx, w) in taps {
                for c in 0..4 {
                    dst[ox * 4 + c] += f32::from(src[sx * 4 + c]) * w;
                }
            }
        }
    }
    let mut out = vec![0u8; (out_w * out_h * 4) as usize];
    let stride = out_w as usize * 4;
    for (oy, taps) in y_weights.iter().enumerate() {
        for i in 0..stride {
            let v: f32 = taps.iter().map(|&(sy, w)| rows[sy * stride + i] * w).sum();
            out[oy * stride + i] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    (out_w, out_h, out)
}

/// For each output index, the source indices it covers and their weights
/// (summing to 1).
fn coverage(src: u32, dst: u32) -> Vec<Vec<(usize, f32)>> {
    let scale = src as f64 / dst as f64;
    (0..dst)
        .map(|o| {
            let start = o as f64 * scale;
            let end = (start + scale).min(src as f64);
            let mut taps = Vec::new();
            let mut s = start.floor() as usize;
            while (s as f64) < end {
                let lo = (s as f64).max(start);
                let hi = ((s + 1) as f64).min(end);
                if hi > lo {
                    taps.push((s, ((hi - lo) / scale) as f32));
                }
                s += 1;
            }
            taps
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_edge_doubles_onto_stock_sizes() {
        assert_eq!(fetch_edge(16), 48);
        assert_eq!(fetch_edge(22), 48);
        assert_eq!(fetch_edge(33), 96);
        assert_eq!(fetch_edge(44), 96);
        assert_eq!(fetch_edge(66), 256);
        assert_eq!(fetch_edge(256), 256);
    }

    #[test]
    fn trims_a_small_icon_centered_in_a_large_canvas() {
        // 48×48 art centered in 96×96, like the shell's upsized 7-Zip icon.
        let mut px = vec![0u8; 96 * 96 * 4];
        for y in 24..72 {
            for x in 24..72 {
                px[(y * 96 + x) * 4..][..4].copy_from_slice(&[9, 9, 9, 255]);
            }
        }
        let (w, h, out) = trim_padding(96, 96, &px);
        assert_eq!((w, h), (48, 48));
        assert!(out.as_chunks::<4>().0.iter().all(|p| *p == [9, 9, 9, 255]));
    }

    #[test]
    fn keeps_designed_padding() {
        // Art covering 80% of the edge is a normal icon.
        let mut px = vec![0u8; 50 * 50 * 4];
        for y in 5..45 {
            for x in 5..45 {
                px[(y * 50 + x) * 4 + 3] = 255;
            }
        }
        assert_eq!(trim_padding(50, 50, &px).0, 50);
        assert_eq!(trim_padding(50, 50, &vec![0u8; 50 * 50 * 4]).0, 50);
    }

    #[test]
    fn non_square_art_wider_than_the_image_passes_through() {
        // A 31-row column of art in a 4×100 strip fits no 31×31 square.
        let mut px = vec![0u8; 4 * 100 * 4];
        for y in 69..100 {
            for x in 1..4 {
                px[(y * 4 + x) * 4 + 3] = 255;
            }
        }
        assert_eq!(trim_padding(4, 100, &px), (4, 100, px.clone()));
    }

    #[test]
    fn small_images_pass_through() {
        let px = vec![7u8; 4 * 4 * 4];
        assert_eq!(downscale_premultiplied(4, 4, &px, 8), (4, 4, px));
    }

    #[test]
    fn halving_averages_blocks() {
        // 2×2 → 1×1: one opaque white pixel, three transparent.
        let mut px = vec![0u8; 16];
        px[..4].copy_from_slice(&[255, 255, 255, 255]);
        let (w, h, out) = downscale_premultiplied(2, 2, &px, 1);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out, vec![64, 64, 64, 64]);
    }

    #[test]
    fn fractional_scale_preserves_flat_color() {
        let px = [10u8, 20, 30, 255].repeat(48 * 48);
        let (w, h, out) = downscale_premultiplied(48, 48, &px, 22);
        assert_eq!((w, h), (22, 22));
        assert!(
            out.as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [10, 20, 30, 255])
        );
    }

    #[test]
    fn aspect_ratio_is_kept() {
        let px = vec![0u8; 64 * 32 * 4];
        let (w, h, out) = downscale_premultiplied(64, 32, &px, 22);
        assert_eq!((w, h), (22, 11));
        assert_eq!(out.len(), 22 * 11 * 4);
    }
}
