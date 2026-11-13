use std::{io, time::Duration};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};
use tuilane_core::{DuplicateViewId, ViewRegistry};
use tuilane_plugin_api::ViewId;

use crate::View;

/// The shell region currently receiving keyboard navigation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FocusArea {
    Navigation,
    #[default]
    Content,
    Inspector,
    Drawer,
}

/// Error returned when a workspace contribution cannot be registered.
#[derive(Debug)]
pub enum WorkspaceError {
    DuplicateView(DuplicateViewId),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateView(error) => {
                write!(formatter, "view '{}' is already registered", error.0)
            }
        }
    }
}

impl std::error::Error for WorkspaceError {}

/// Host-owned shell state and registered view contributions.
pub struct Workspace {
    views: ViewRegistry<Box<dyn View>>,
    selected: Option<usize>,
    focus: FocusArea,
    inspector_open: bool,
    drawer_open: bool,
    help_open: bool,
    should_quit: bool,
    last_layout: ShellLayout,
}

#[derive(Clone, Copy, Debug, Default)]
struct ShellLayout {
    navigation: bool,
    inspector: bool,
    drawer: bool,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            views: ViewRegistry::new(),
            selected: None,
            focus: FocusArea::Content,
            inspector_open: false,
            drawer_open: false,
            help_open: false,
            should_quit: false,
            last_layout: ShellLayout::default(),
        }
    }
}

impl Workspace {
    /// Creates an empty workspace shell.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a view without requiring a central screen switch.
    pub fn register_view<V: View + 'static>(&mut self, view: V) -> Result<(), WorkspaceError> {
        let descriptor = view.descriptor().clone();
        self.views
            .register(descriptor, Box::new(view))
            .map_err(WorkspaceError::DuplicateView)?;
        if self.selected.is_none() {
            self.selected = Some(0);
        }
        Ok(())
    }

    /// Removes a view by stable identifier and returns its implementation.
    pub fn remove_view(&mut self, id: &ViewId) -> Option<Box<dyn View>> {
        let removed_index = self
            .views
            .iter()
            .position(|view| view.descriptor().id() == id)?;
        let removed = self.views.remove(id)?.into_value();

        self.selected = match (self.selected, self.views.is_empty()) {
            (_, true) => None,
            (Some(selected), false) if selected > removed_index => Some(selected - 1),
            (Some(selected), false) if selected == removed_index => {
                Some(selected.min(self.views.len() - 1))
            }
            (selected, false) => selected,
        };
        if self.views.is_empty() {
            self.focus = FocusArea::Content;
            self.inspector_open = false;
            self.drawer_open = false;
        }

        Some(removed)
    }

    /// Returns the number of registered views.
    pub fn view_count(&self) -> usize {
        self.views.len()
    }

    /// Returns the currently selected view identifier, if any.
    pub fn selected_view_id(&self) -> Option<&ViewId> {
        self.selected
            .and_then(|index| self.views.get(index))
            .map(|view| view.descriptor().id())
    }

    /// Returns the active focus region.
    pub fn focus(&self) -> FocusArea {
        self.focus
    }

    /// Opens or closes the inspector panel.
    pub fn toggle_inspector(&mut self) {
        self.inspector_open = !self.inspector_open;
        if self.inspector_open {
            self.focus = FocusArea::Inspector;
        } else if self.focus == FocusArea::Inspector {
            self.focus = FocusArea::Content;
        }
    }

    /// Opens or closes the bottom drawer.
    pub fn toggle_drawer(&mut self) {
        self.drawer_open = !self.drawer_open;
        if self.drawer_open {
            self.focus = FocusArea::Drawer;
        } else if self.focus == FocusArea::Drawer {
            self.focus = FocusArea::Content;
        }
    }

    /// Returns whether the help overlay is visible.
    pub fn help_is_open(&self) -> bool {
        self.help_open
    }

    /// Returns whether the event loop should exit.
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Routes one key event to the focused shell region.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }

        if self.help_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => self.help_open = false,
                KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return;
        }

        let view_consumed = self
            .selected
            .and_then(|index| self.views.get_mut(index))
            .is_some_and(|view| view.value_mut().handle_key(key, self.focus));
        if view_consumed {
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.help_open = true,
            KeyCode::Esc => self.focus = FocusArea::Content,
            KeyCode::Char('i') => self.toggle_inspector(),
            KeyCode::Char('d') => self.toggle_drawer(),
            KeyCode::Tab => self.advance_focus(false),
            KeyCode::BackTab => self.advance_focus(true),
            KeyCode::Up | KeyCode::Char('k') if self.focus == FocusArea::Navigation => {
                self.select_previous()
            }
            KeyCode::Down | KeyCode::Char('j') if self.focus == FocusArea::Navigation => {
                self.select_next()
            }
            KeyCode::Left | KeyCode::Char('h') => self.select_previous(),
            KeyCode::Right | KeyCode::Char('l') => self.select_next(),
            _ => {}
        }
    }

    /// Draws the responsive workspace shell and its registered contributions.
    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let layout = Self::layout(area, self.inspector_open, self.drawer_open);
        self.last_layout = layout.state;
        self.normalize_focus(layout.state);

        let current_descriptor = self
            .selected
            .and_then(|index| self.views.get(index))
            .map(|view| view.descriptor().clone());
        let header_title = current_descriptor
            .as_ref()
            .map_or("Workspace".to_owned(), |view| view.title().to_owned());
        frame.render_widget(
            Paragraph::new(format!("tuilane  /  {header_title}"))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            layout.header,
        );

        if let Some(navigation) = layout.navigation {
            self.render_navigation(frame, navigation);
        }
        self.render_content(frame, layout.main);
        if let Some(inspector) = layout.inspector {
            self.render_inspector(frame, inspector);
        }
        if let Some(drawer) = layout.drawer {
            self.render_drawer(frame, drawer);
        }

        frame.render_widget(
            Paragraph::new(
                "Tab focus  h/l views  ↑/↓ in Views  i inspector  d drawer  ? help  q quit",
            ),
            layout.status,
        );

        if self.help_open {
            self.render_help(frame, area);
        }
    }

    fn render_navigation(&self, frame: &mut Frame<'_>, area: Rect) {
        let items = self
            .views
            .iter()
            .enumerate()
            .map(|(index, view)| {
                let marker = if Some(index) == self.selected {
                    "> "
                } else {
                    "  "
                };
                Line::from(format!("{marker}{}", view.descriptor().title()))
            })
            .collect::<Vec<_>>();
        let block = Self::panel_block("Views", self.focus == FocusArea::Navigation);
        frame.render_widget(Paragraph::new(items).block(block), area);
    }

    fn render_content(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let Some(index) = self.selected else {
            let block = Self::panel_block("Workspace", self.focus == FocusArea::Content);
            frame.render_widget(
                Paragraph::new("No views are currently registered.")
                    .block(block)
                    .wrap(ratatui::widgets::Wrap { trim: true }),
                area,
            );
            return;
        };
        let Some(view) = self.views.get_mut(index) else {
            return;
        };
        let block = Self::panel_block(view.descriptor().title(), self.focus == FocusArea::Content);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        view.value_mut().render(frame, inner);
    }

    fn render_inspector(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let block = Self::panel_block("Inspector", self.focus == FocusArea::Inspector);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let rendered = self
            .selected
            .and_then(|index| self.views.get_mut(index))
            .is_some_and(|view| view.value_mut().render_inspector(frame, inner));
        if !rendered {
            frame.render_widget(
                Paragraph::new("No contextual details for this view.")
                    .wrap(ratatui::widgets::Wrap { trim: true }),
                inner,
            );
        }
    }

    fn render_drawer(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let block = Self::panel_block("Drawer", self.focus == FocusArea::Drawer);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let rendered = self
            .selected
            .and_then(|index| self.views.get_mut(index))
            .is_some_and(|view| view.value_mut().render_drawer(frame, inner));
        if !rendered {
            frame.render_widget(
                Paragraph::new("No supplemental content for this view.")
                    .wrap(ratatui::widgets::Wrap { trim: true }),
                inner,
            );
        }
    }

    fn render_help(&self, frame: &mut Frame<'_>, area: Rect) {
        let width = area.width.saturating_sub(4).min(64);
        let height = area.height.saturating_sub(2).min(11);
        let overlay = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, overlay);
        let text = Text::from(vec![
            Line::from("Workspace keys"),
            Line::from("h/l  Move between views; ↑/↓ when Views has focus"),
            Line::from("Tab / Shift-Tab  Move focus between visible panels"),
            Line::from("i  Toggle inspector     d  Toggle drawer"),
            Line::from("? or Esc  Close this overlay     q  Quit"),
        ]);
        frame.render_widget(
            Paragraph::new(text)
                .block(Self::panel_block("Help", true))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            overlay,
        );
    }

    fn panel_block<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
        let title = title.into();
        let title = if focused {
            Line::from(format!("{title} [focus]"))
        } else {
            title
        };
        Block::default().borders(Borders::ALL).title(title)
    }

    fn layout(area: Rect, inspector_open: bool, drawer_open: bool) -> ShellRects {
        let [header, body, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);

        let show_drawer = drawer_open && area.height >= 12;
        let (body, drawer) = if show_drawer {
            let [main, drawer] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(6)]).areas(body);
            (main, Some(drawer))
        } else {
            (body, None)
        };

        let show_inspector = inspector_open && area.width >= 52;
        let show_navigation = area.width >= 52 && (!show_inspector || area.width >= 84);
        let state = ShellLayout {
            navigation: show_navigation,
            inspector: show_inspector,
            drawer: show_drawer,
        };
        let (navigation, main, inspector) = match (show_navigation, show_inspector) {
            (true, true) => {
                let [navigation, main, inspector] = Layout::horizontal([
                    Constraint::Length(22),
                    Constraint::Min(20),
                    Constraint::Length(28),
                ])
                .areas(body);
                (Some(navigation), main, Some(inspector))
            }
            (true, false) => {
                let [navigation, main] =
                    Layout::horizontal([Constraint::Length(22), Constraint::Min(20)]).areas(body);
                (Some(navigation), main, None)
            }
            (false, true) => {
                let [main, inspector] =
                    Layout::horizontal([Constraint::Min(20), Constraint::Length(28)]).areas(body);
                (None, main, Some(inspector))
            }
            (false, false) => (None, body, None),
        };

        ShellRects {
            header,
            navigation,
            main,
            inspector,
            drawer,
            status,
            state,
        }
    }

    fn advance_focus(&mut self, backwards: bool) {
        let mut order = Vec::with_capacity(4);
        if self.last_layout.navigation {
            order.push(FocusArea::Navigation);
        }
        order.push(FocusArea::Content);
        if self.inspector_open && self.last_layout.inspector {
            order.push(FocusArea::Inspector);
        }
        if self.drawer_open && self.last_layout.drawer {
            order.push(FocusArea::Drawer);
        }
        let current = order
            .iter()
            .position(|focus| *focus == self.focus)
            .unwrap_or(0);
        let next = if backwards {
            (current + order.len() - 1) % order.len()
        } else {
            (current + 1) % order.len()
        };
        self.focus = order[next];
    }

    fn normalize_focus(&mut self, layout: ShellLayout) {
        let visible = match self.focus {
            FocusArea::Navigation => layout.navigation,
            FocusArea::Content => true,
            FocusArea::Inspector => layout.inspector,
            FocusArea::Drawer => layout.drawer,
        };
        if !visible {
            self.focus = FocusArea::Content;
        }
    }

    fn select_previous(&mut self) {
        if let Some(index) = self.selected.filter(|_| !self.views.is_empty()) {
            self.selected = Some(if index == 0 {
                self.views.len() - 1
            } else {
                index - 1
            });
        }
    }

    fn select_next(&mut self) {
        if let Some(index) = self.selected.filter(|_| !self.views.is_empty()) {
            self.selected = Some((index + 1) % self.views.len());
        }
    }
}

struct ShellRects {
    header: Rect,
    navigation: Option<Rect>,
    main: Rect,
    inspector: Option<Rect>,
    drawer: Option<Rect>,
    status: Rect,
    state: ShellLayout,
}

/// Starts the workspace event loop, managing terminal setup and restoration.
pub fn run(mut workspace: Workspace) -> io::Result<()> {
    require_interactive_terminal()?;

    ratatui::run(|terminal| {
        'redraw: loop {
            terminal.draw(|frame| workspace.render(frame))?;
            loop {
                if !event::poll(Duration::from_secs(30))? {
                    continue;
                }

                match event::read()? {
                    Event::Key(key) => {
                        workspace.handle_key(key);
                        if workspace.should_quit() {
                            break 'redraw;
                        }
                        break;
                    }
                    Event::Resize(_, _) => break,
                    _ => {}
                }
            }
        }
        Ok(())
    })
}

/// Fails when standard input or output is not attached to an interactive terminal.
pub fn require_interactive_terminal() -> io::Result<()> {
    use std::io::IsTerminal;

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "an interactive terminal is required to launch the workspace",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_hides_panels_on_small_terminals() {
        let layout = Workspace::layout(Rect::new(0, 0, 40, 10), true, true);
        assert!(!layout.state.navigation);
        assert!(!layout.state.inspector);
        assert!(!layout.state.drawer);
    }

    #[test]
    fn layout_shows_optional_panels_when_space_allows() {
        let layout = Workspace::layout(Rect::new(0, 0, 120, 36), true, true);
        assert!(layout.state.navigation);
        assert!(layout.state.inspector);
        assert!(layout.state.drawer);
    }

    #[test]
    fn inspector_uses_the_available_width_before_navigation() {
        let layout = Workspace::layout(Rect::new(0, 0, 60, 20), true, false);
        assert!(!layout.state.navigation);
        assert!(layout.state.inspector);
    }

    #[test]
    fn focus_cycle_never_targets_hidden_panels() {
        let mut workspace = Workspace::new();
        workspace.last_layout = ShellLayout::default();
        workspace.toggle_inspector();
        workspace.advance_focus(false);
        assert_eq!(workspace.focus(), FocusArea::Content);
    }
}
