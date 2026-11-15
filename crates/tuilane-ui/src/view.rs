use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
};
use tuilane_plugin_api::{DashboardLayout, DashboardWidget, DashboardWidgetId, ViewDescriptor};

/// Renders a centered title and a short next-step message for an empty view.
pub fn render_empty_state(frame: &mut Frame<'_>, area: Rect, title: &str, detail: &str) {
    if area.is_empty() {
        return;
    }

    let lines = vec![
        Line::from(Span::styled(
            title.to_owned(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(detail.to_owned()),
    ];
    let height = area.height.min(lines.len() as u16);
    let message_area = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(height) / 2,
        area.width,
        height,
    );
    frame.render_widget(
        Paragraph::new(Text::from(lines)).alignment(Alignment::Center),
        message_area,
    );
}

/// One key and action pair a view displays in the workspace footer and help.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Keybinding {
    action: Box<str>,
    key: Box<str>,
    priority: u8,
}

impl Keybinding {
    /// Creates a contextual shortcut description for a view.
    pub fn new(action: impl Into<Box<str>>, key: impl Into<Box<str>>) -> Self {
        Self {
            action: action.into(),
            key: key.into(),
            priority: 50,
        }
    }

    /// Sets how prominently this shortcut appears in the one-line footer.
    ///
    /// Higher values are shown first; equal priorities keep the order returned
    /// by the view. Every shortcut remains available in the help overlay.
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Returns the action label shown to the user.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns the key label shown to the user.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns this shortcut's footer display priority.
    pub fn priority(&self) -> u8 {
        self.priority
    }
}

/// One action a view contributes to the workspace command palette.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewAction {
    id: Box<str>,
    title: Box<str>,
    description: Box<str>,
}

/// A host-rendered dialog contributed by the active view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViewDialog {
    /// Requests one text value from the user.
    Input {
        /// Short dialog title.
        title: String,
        /// Label describing the value being entered.
        label: String,
        /// Current editable value.
        value: String,
        /// Validation feedback shown while the dialog remains open.
        error: Option<String>,
    },
    /// Requests confirmation before a destructive action.
    Confirm {
        /// Short dialog title.
        title: String,
        /// Confirmation question or target description.
        message: String,
    },
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

    /// Returns the compact icon centered beside this view's tab title.
    ///
    /// The shell centers it in a three-cell slot; icons should occupy no more than
    /// three terminal columns. The title remains visible as the view's identifier.
    fn icon(&self) -> &'static str {
        crate::icons::DEFAULT
    }

    /// Renders the view's primary content.
    fn render(&mut self, frame: &mut Frame<'_>, area: Rect);

    /// Renders the primary content with the host's active theme.
    ///
    /// The default keeps existing views compatible; shared UI components should
    /// override this hook when they need to honor the selected shell theme.
    fn render_with_theme(&mut self, frame: &mut Frame<'_>, area: Rect, _theme: super::ThemePreset) {
        self.render(frame, area);
    }

    /// Returns host-rendered widgets contributed to the Overview dashboard.
    ///
    /// The host owns layout, input, and rendering. Implementations should return
    /// bounded snapshots and use saved preferences when preparing their content.
    fn dashboard_widgets(&mut self, _layout: &DashboardLayout) -> Vec<DashboardWidget> {
        Vec::new()
    }

    /// Marks this view as the host-rendered dashboard entry point.
    fn is_dashboard(&self) -> bool {
        false
    }

    /// Opens a widget's selected item or focused state after dashboard activation.
    fn open_dashboard_widget(&mut self, _id: &DashboardWidgetId) {}

    /// Handles a key event before the shell routes it to global navigation.
    ///
    /// Return `true` when the view consumes the event. Views should use arrow keys
    /// for in-view vertical navigation. The host offers `j`/`k` as aliases for the
    /// corresponding arrows after giving the original character to the view, so
    /// text inputs can still receive those characters.
    fn handle_key(&mut self, _key: KeyEvent, _focus: super::FocusArea) -> bool {
        false
    }

    /// Handles pointer interaction in the active view's content region.
    fn handle_mouse(&mut self, _event: MouseEvent, _focus: super::FocusArea) -> bool {
        false
    }

    /// Returns shortcuts that are available in the view's current state.
    ///
    /// Implementations can change the returned list while editing or confirming
    /// an action so the footer and help describe the controls that currently work.
    fn keybindings(&self, _focus: super::FocusArea) -> Vec<Keybinding> {
        Vec::new()
    }

    /// Returns a modal dialog for the host to render over the workspace.
    fn dialog(&self) -> Option<ViewDialog> {
        None
    }

    /// Transfers one user-facing notification to the host shell.
    fn take_notification(&mut self) -> Option<String> {
        None
    }

    /// Renders contextual details when the inspector is open.
    ///
    /// Return `true` when this view draws inspector content. The shell displays a
    /// plain fallback when no contribution is available.
    fn render_inspector(&mut self, _frame: &mut Frame<'_>, _area: Rect) -> bool {
        false
    }

    /// Renders supplemental content when the supplemental side panel is open.
    ///
    /// Return `true` when this view draws supplemental content. The shell displays a
    /// plain fallback when no contribution is available.
    fn render_drawer(&mut self, _frame: &mut Frame<'_>, _area: Rect) -> bool {
        false
    }

    /// Returns whether the workspace should redraw periodically for this view's
    /// background work. The view must drain completed work during `render`.
    fn has_background_work(&self) -> bool {
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
