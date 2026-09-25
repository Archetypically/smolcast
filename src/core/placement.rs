//! Palette placement math (plan §12.7): horizontally centered in the monitor
//! work area and raised above center, Raycast-style. Pure and unit-agnostic;
//! the Windows layer feeds physical pixels.

/// An axis-aligned rectangle; `right`/`bottom` are exclusive, like Win32 `RECT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Top-left corner for a `width`×`height` window in `work`. The free vertical
/// space is split 1:2 above and below, so the palette sits about a third of
/// the way down. A window larger than the work area is pinned to its top-left
/// edge so the search field stays reachable.
pub fn palette_origin(work: Rect, width: i32, height: i32) -> (i32, i32) {
    let free_x = (work.width() - width).max(0);
    let free_y = (work.height() - height).max(0);
    (work.left + free_x / 2, work.top + free_y / 3)
}

/// Scale a physical length between DPIs, rounding to nearest.
pub fn rescale(length: i32, from_dpi: u32, to_dpi: u32) -> i32 {
    if from_dpi == 0 || from_dpi == to_dpi {
        return length;
    }
    let scaled = i64::from(length) * i64::from(to_dpi);
    let from = i64::from(from_dpi);
    ((scaled + from / 2) / from) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn centers_horizontally_and_raises_vertically() {
        // 1920x1040 work area (taskbar at the bottom), 560x420 window.
        let origin = palette_origin(rect(0, 0, 1920, 1040), 560, 420);
        assert_eq!(origin, (680, 206));
    }

    #[test]
    fn respects_offset_monitors() {
        // A secondary monitor left of and above the primary.
        let origin = palette_origin(rect(-2560, -300, 0, 1140), 840, 630);
        assert_eq!(origin, (-2560 + 860, -300 + 270));
    }

    #[test]
    fn oversized_window_pins_to_work_origin() {
        assert_eq!(palette_origin(rect(100, 50, 600, 400), 800, 600), (100, 50));
    }

    #[test]
    fn rescales_between_dpis() {
        assert_eq!(rescale(560, 96, 144), 840);
        assert_eq!(rescale(840, 144, 192), 1120);
        assert_eq!(rescale(1120, 192, 96), 560);
        assert_eq!(rescale(421, 144, 96), 281); // 280.67 rounds up
        assert_eq!(rescale(560, 0, 144), 560);
    }
}
