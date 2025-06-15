use ratatui::style::Color;

/// Global colour palette used across the TUI.
/// The values are chosen to be accessible and work in both light & dark terminals.
pub struct Palette;

impl Palette {
    /// Normal (unselected) text colour.
    pub const NORMAL: Color = Color::White;

    /// Colour for the currently selected row.
    pub const SELECTED_BG: Color = Color::Indexed(25); // blue
    pub const SELECTED_FG: Color = Color::White;

    /// Dimmed colour for hidden/greyed entries.
    pub const HIDDEN: Color = Color::Indexed(241);

    /// Breadcrumb foreground colour.
    pub const BREADCRUMB: Color = Color::Yellow;

    /// Footer hint bar foreground.
    pub const FOOTER: Color = Color::Indexed(244);
}
