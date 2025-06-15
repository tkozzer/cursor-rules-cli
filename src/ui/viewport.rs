//! Virtualised tree viewport stub.

/// Viewport is responsible for keeping track of the portion of the repository tree currently
/// visible on screen and ensuring the selected item stays within bounds. Full implementation
/// will arrive in subsequent iterations.
#[derive(Default)]
pub struct Viewport {
    pub scroll_offset: usize,
    pub selected_index: usize,
}

impl Viewport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Move selection up one item.
    pub fn up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down one item.
    pub fn down(&mut self, total_items: usize) {
        if self.selected_index + 1 < total_items {
            self.selected_index += 1;
        }
    }

    /// Adjusts scroll offset so that the selected item stays within visible range.
    pub fn ensure_visible(&mut self, view_height: usize) {
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + view_height {
            self.scroll_offset = self.selected_index + 1 - view_height;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_down_and_visibility() {
        let mut vp = Viewport::new();
        // initial selected 0
        vp.down(10);
        assert_eq!(vp.selected_index, 1);
        vp.up();
        assert_eq!(vp.selected_index, 0);

        // test ensure_visible
        vp.selected_index = 15;
        vp.ensure_visible(5); // height 5
        assert!(vp.scroll_offset <= vp.selected_index);
        assert!(vp.selected_index < vp.scroll_offset + 5);
    }
}
