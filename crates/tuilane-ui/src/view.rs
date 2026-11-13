use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect};
use tuilane_plugin_api::ViewDescriptor;

/// A renderable contribution registered in the host workspace.
pub trait View {
    /// Returns stable identity and display metadata for this view.
    fn descriptor(&self) -> &ViewDescriptor;

    /// Renders the view's primary content.
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect);

    /// Handles a key event before the shell routes it to global navigation.
    ///
    /// Return `true` when the view consumes the event.
    fn handle_key(&mut self, _key: KeyEvent, _focus: super::FocusArea) -> bool {
        false
    }

    /// Renders contextual details when the inspector is open.
    ///
    /// Return `true` when this view draws inspector content. The shell displays a
    /// plain fallback when no contribution is available.
    fn render_inspector(&mut self, _frame: &mut Frame<'_>, _area: Rect) -> bool {
        false
    }

    /// Renders supplemental content when the bottom drawer is open.
    ///
    /// Return `true` when this view draws drawer content. The shell displays a
    /// plain fallback when no contribution is available.
    fn render_drawer(&mut self, _frame: &mut Frame<'_>, _area: Rect) -> bool {
        false
    }
}
