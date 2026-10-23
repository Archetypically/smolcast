//! Selection and scroll position of the palette's windowed lists: wrapping
//! arrow-key selection, the visible-row window, and mouse-wheel carry.

/// Where a windowed list is: the selected row, the first visible row, and the
/// wheel distance not yet worth a whole row.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ListCursor {
    pub selected: usize,
    pub first_row: usize,
    pub wheel: f32,
}

impl ListCursor {
    /// Back to the top.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Move the selection one row down or up, wrapping at both ends.
    pub fn step(&mut self, count: usize, down: bool, up: bool) {
        if count == 0 {
            return;
        }
        if down {
            self.selected = (self.selected + 1) % count;
        }
        if up {
            self.selected = (self.selected + count - 1) % count;
        }
    }

    /// Keep the selection on one of `count` rows, e.g. after the list shrank.
    pub fn clamp_selected(&mut self, count: usize) {
        self.selected = self.selected.min(count.saturating_sub(1));
    }

    /// Scroll a plain `visible`-row window over `count` rows by `step` rows,
    /// then keep the selection inside it. Returns the visible rows.
    pub fn scroll(&mut self, count: usize, visible: usize, step: isize) -> std::ops::Range<usize> {
        self.selected = scroll_window(&mut self.first_row, self.selected, count, visible, step);
        keep_visible(self.selected, &mut self.first_row, visible);
        self.first_row..(self.first_row + visible).min(count)
    }
}

/// Scroll the `visible_rows` window starting at `first_row` so it shows
/// `selected`.
pub fn keep_visible(selected: usize, first_row: &mut usize, visible_rows: usize) {
    if selected < *first_row {
        *first_row = selected;
    } else if selected >= *first_row + visible_rows {
        *first_row = selected + 1 - visible_rows;
    }
}

/// Move a `visible`-row window over `count` rows by `step` rows, then keep
/// `selected` inside it, so the selection follows the wheel instead of
/// pulling the window back. Returns the selection to use.
pub fn scroll_window(
    first_row: &mut usize,
    selected: usize,
    count: usize,
    visible: usize,
    step: isize,
) -> usize {
    // Clamp even without a scroll: a list that shrank under the window
    // would otherwise leave blank rows below its end.
    let max_first = count.saturating_sub(visible);
    *first_row = (*first_row).min(max_first);
    if step == 0 || count == 0 {
        return selected;
    }
    *first_row = first_row.saturating_add_signed(step).min(max_first);
    let last = (*first_row + visible).min(count) - 1;
    selected.clamp(*first_row, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(selected: usize, first_row: usize) -> ListCursor {
        ListCursor {
            selected,
            first_row,
            wheel: 0.0,
        }
    }

    #[test]
    fn step_wraps_both_ways_and_ignores_empty_lists() {
        let mut c = at(2, 0);
        c.step(3, true, false);
        assert_eq!(c.selected, 0);
        c.step(3, false, true);
        assert_eq!(c.selected, 2);
        c.step(0, true, false);
        assert_eq!(c.selected, 2);
    }

    #[test]
    fn scroll_follows_the_selection() {
        let mut c = at(7, 0);
        assert_eq!(c.scroll(20, 5, 0), 3..8);
        c.selected = 1;
        assert_eq!(c.scroll(20, 5, 0), 1..6);
    }

    #[test]
    fn wheel_moves_the_window_and_drags_the_selection() {
        let mut c = at(0, 0);
        assert_eq!(c.scroll(20, 5, 3), 3..8);
        assert_eq!(c.selected, 3);
        assert_eq!(c.scroll(20, 5, 100), 15..20);
        assert_eq!(c.selected, 15);
        assert_eq!(c.scroll(20, 5, -100), 0..5);
        assert_eq!(c.selected, 4);
    }

    #[test]
    fn a_shrunken_list_pulls_the_window_back() {
        let mut c = at(2, 10);
        assert_eq!(c.scroll(4, 5, 0), 0..4);
        assert_eq!(c.scroll(0, 5, 2), 0..0);
    }

    #[test]
    fn reset_returns_to_the_top() {
        let mut c = ListCursor {
            selected: 4,
            first_row: 2,
            wheel: 7.5,
        };
        c.reset();
        assert_eq!(c, ListCursor::default());
    }
}
