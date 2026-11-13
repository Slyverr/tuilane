use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect};
use tuilane_plugin_api::ViewDescriptor;

/// One action a view contributes to the workspace command palette.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewAction {
    id: Box<str>,
    title: Box<str>,
    description: Box<str>,
}

impl ViewAction {
    /// Creates an action with a stable view-local identifier and searchable label.
    pub fn new(
        id: impl Into<Box<str>>,
        title: impl Into<Box<str>>,
        description: impl Into<Box<str>>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
        }
    }

    /// Returns the stable identifier used to activate this action.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the label displayed in the palette.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns searchable help text for the action.
    pub fn description(&self) -> &str {
        &self.description
    }
}

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

    /// Returns actions this view contributes to the command palette.
    fn command_palette_actions(&self) -> Vec<ViewAction> {
        Vec::new()
    }

    /// Activates one command-palette action by its view-local identifier.
    fn activate_command_palette_action(&mut self, _id: &str) -> bool {
        false
    }
}
