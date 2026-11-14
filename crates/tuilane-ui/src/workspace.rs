use std::{io, time::Duration};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};
use tuilane_core::{DuplicateViewId, ViewRegistry};
use tuilane_plugin_api::ViewId;

use crate::{KeymapPreset, LayoutPreset, ThemePreset, View, WorkspaceConfig};

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
    palette_open: bool,
    palette_query: String,
    palette_selected: usize,
    should_quit: bool,
    config: WorkspaceConfig,
    last_layout: ShellLayout,
}

enum PaletteAction {
    SelectView(ViewId),
    ActivateViewAction {
        view_id: ViewId,
        action_id: Box<str>,
    },
    ToggleInspector,
    ToggleDrawer,
    ShowHelp,
    Quit,
}

struct PaletteEntry {
    label: String,
    search_text: String,
    action: PaletteAction,
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
            palette_open: false,
            palette_query: String::new(),
            palette_selected: 0,
            should_quit: false,
            config: WorkspaceConfig::default(),
            last_layout: ShellLayout::default(),
        }
    }
}

impl Workspace {
    /// Creates an empty workspace shell.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty workspace shell using the supplied presentation presets.
    pub fn with_config(config: WorkspaceConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
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

    /// Returns whether the command palette is visible.
    pub fn command_palette_is_open(&self) -> bool {
        self.palette_open
    }

    /// Returns whether the event loop should exit.
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    fn selected_view_has_background_work(&self) -> bool {
        self.selected
            .and_then(|index| self.views.get(index))
            .is_some_and(|view| view.value().has_background_work())
    }

    /// Routes one key event to the focused shell region.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.toggle_command_palette();
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }

        if self.palette_open {
            self.handle_palette_key(key);
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
            KeyCode::Up if self.focus == FocusArea::Navigation => self.select_previous(),
            KeyCode::Down if self.focus == FocusArea::Navigation => self.select_next(),
            KeyCode::Char('k') if self.config.keymap() == KeymapPreset::Vim => {
                self.select_previous()
            }
            KeyCode::Char('j') if self.config.keymap() == KeymapPreset::Vim => self.select_next(),
            KeyCode::Up => self.select_previous(),
            KeyCode::Down => self.select_next(),
            _ => {}
        }
    }

    /// Draws the responsive workspace shell and its registered contributions.
    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let layout = Self::layout(
            area,
            self.inspector_open,
            self.drawer_open,
            self.config.layout(),
        );
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
                .style(self.config.theme().header_style()),
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

        let navigation_keys = if self.config.keymap() == KeymapPreset::Vim {
            "↑/↓ j/k views"
        } else {
            "↑/↓ views"
        };
        frame.render_widget(
            Paragraph::new(format!(
                "Tab focus  {navigation_keys}  Ctrl-P commands  i inspector  d drawer  ? help  q quit"
            )),
            layout.status,
        );

        if self.help_open {
            self.render_help(frame, area);
        }
        if self.palette_open {
            self.render_command_palette(frame, area);
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
                let line = Line::from(format!("{marker}{}", view.descriptor().title()));
                if Some(index) == self.selected {
                    line.style(self.config.theme().panel_style(true))
                } else {
                    line
                }
            })
            .collect::<Vec<_>>();
        let block = Self::panel_block(
            "Views",
            self.focus == FocusArea::Navigation,
            self.config.theme(),
        );
        frame.render_widget(Paragraph::new(items).block(block), area);
    }

    fn render_content(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let Some(index) = self.selected else {
            let block = Self::panel_block(
                "Workspace",
                self.focus == FocusArea::Content,
                self.config.theme(),
            );
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
        let block = Self::panel_block(
            view.descriptor().title(),
            self.focus == FocusArea::Content,
            self.config.theme(),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        view.value_mut().render(frame, inner);
    }

    fn render_inspector(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let block = Self::panel_block(
            "Inspector",
            self.focus == FocusArea::Inspector,
            self.config.theme(),
        );
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
        let block = Self::panel_block(
            "Drawer",
            self.focus == FocusArea::Drawer,
            self.config.theme(),
        );
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
        let navigation_keys = if self.config.keymap() == KeymapPreset::Vim {
            "↑/↓ or j/k  Move views when unhandled"
        } else {
            "↑/↓  Move views when unhandled"
        };
        let text = Text::from(vec![
            Line::from("Workspace keys"),
            Line::from(navigation_keys),
            Line::from("Tab / Shift-Tab  Move focus between visible panels"),
            Line::from("i  Toggle inspector     d  Toggle drawer"),
            Line::from("Ctrl-P  Open the command palette"),
            Line::from("? or Esc  Close this overlay     q  Quit"),
        ]);
        frame.render_widget(
            Paragraph::new(text)
                .block(Self::panel_block("Help", true, self.config.theme()))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            overlay,
        );
    }

    fn toggle_command_palette(&mut self) {
        self.palette_open = !self.palette_open;
        self.palette_query.clear();
        self.palette_selected = 0;
    }

    fn handle_palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.palette_open = false,
            KeyCode::Enter => self.activate_palette_selection(),
            KeyCode::Up => self.move_palette_selection(false),
            KeyCode::Down => self.move_palette_selection(true),
            KeyCode::Backspace => {
                self.palette_query.pop();
                self.palette_selected = 0;
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.palette_query.push(character);
                self.palette_selected = 0;
            }
            _ => {}
        }
    }

    fn palette_entries(&self) -> Vec<PaletteEntry> {
        let mut entries = self
            .views
            .iter()
            .map(|view| {
                let title = view.descriptor().title();
                let id = view.descriptor().id().clone();
                PaletteEntry {
                    label: format!("Go to {title}"),
                    search_text: format!("{title} {} {}", view.descriptor().description(), id)
                        .to_lowercase(),
                    action: PaletteAction::SelectView(id),
                }
            })
            .collect::<Vec<_>>();
        entries.extend(self.views.iter().flat_map(|view| {
            let descriptor = view.descriptor();
            let view_id = descriptor.id().clone();
            view.value()
                .command_palette_actions()
                .into_iter()
                .map(move |action| PaletteEntry {
                    label: action.title().to_owned(),
                    search_text: format!(
                        "{} {} {} {}",
                        descriptor.title(),
                        action.title(),
                        action.description(),
                        action.id()
                    )
                    .to_lowercase(),
                    action: PaletteAction::ActivateViewAction {
                        view_id: view_id.clone(),
                        action_id: action.id().into(),
                    },
                })
        }));
        entries.extend([
            PaletteEntry {
                label: "Toggle inspector".to_owned(),
                search_text: "toggle inspector details".to_owned(),
                action: PaletteAction::ToggleInspector,
            },
            PaletteEntry {
                label: "Toggle drawer".to_owned(),
                search_text: "toggle drawer supplemental".to_owned(),
                action: PaletteAction::ToggleDrawer,
            },
            PaletteEntry {
                label: "Show help".to_owned(),
                search_text: "show help keyboard shortcuts".to_owned(),
                action: PaletteAction::ShowHelp,
            },
            PaletteEntry {
                label: "Quit workspace".to_owned(),
                search_text: "quit exit workspace".to_owned(),
                action: PaletteAction::Quit,
            },
        ]);
        entries
    }

    fn filtered_palette_entries(&self) -> Vec<PaletteEntry> {
        let query = self.palette_query.to_lowercase();
        self.palette_entries()
            .into_iter()
            .filter(|entry| entry.search_text.contains(&query))
            .collect()
    }

    fn move_palette_selection(&mut self, forward: bool) {
        let count = self.filtered_palette_entries().len();
        if count == 0 {
            self.palette_selected = 0;
        } else if forward {
            self.palette_selected = (self.palette_selected + 1) % count;
        } else {
            self.palette_selected = (self.palette_selected + count - 1) % count;
        }
    }

    fn activate_palette_selection(&mut self) {
        let Some(entry) = self
            .filtered_palette_entries()
            .into_iter()
            .nth(self.palette_selected)
        else {
            return;
        };
        self.palette_open = false;
        match entry.action {
            PaletteAction::SelectView(id) => {
                self.selected = self
                    .views
                    .iter()
                    .position(|view| view.descriptor().id() == &id);
            }
            PaletteAction::ActivateViewAction { view_id, action_id } => {
                let index = self
                    .views
                    .iter()
                    .position(|view| view.descriptor().id() == &view_id);
                if let Some(index) = index {
                    self.selected = Some(index);
                    self.focus = FocusArea::Content;
                    if let Some(view) = self.views.get_mut(index) {
                        view.value_mut().activate_command_palette_action(&action_id);
                    }
                }
            }
            PaletteAction::ToggleInspector => self.toggle_inspector(),
            PaletteAction::ToggleDrawer => self.toggle_drawer(),
            PaletteAction::ShowHelp => self.help_open = true,
            PaletteAction::Quit => self.should_quit = true,
        }
    }

    fn render_command_palette(&self, frame: &mut Frame<'_>, area: Rect) {
        if area.width < 8 || area.height < 4 {
            return;
        }
        let entries = self.filtered_palette_entries();
        let visible_count = entries
            .len()
            .min(8)
            .min(area.height.saturating_sub(5) as usize);
        let width = area.width.saturating_sub(2).min(72);
        let height = if entries.is_empty() {
            4
        } else {
            visible_count as u16 + 3
        }
        .min(area.height.saturating_sub(2));
        let overlay = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        let mut lines = vec![Line::from(format!(" > {}", self.palette_query))];
        if entries.is_empty() {
            lines.push(Line::from("No matching actions"));
        } else {
            let start = self
                .palette_selected
                .saturating_sub(visible_count.saturating_sub(1));
            for (offset, entry) in entries.iter().skip(start).take(visible_count).enumerate() {
                let index = start + offset;
                let marker = if index == self.palette_selected {
                    "> "
                } else {
                    "  "
                };
                lines.push(Line::from(format!("{marker}{}", entry.label)));
            }
        }
        frame.render_widget(Clear, overlay);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(Self::panel_block(
                    "Commands · Enter select · Esc close",
                    true,
                    self.config.theme(),
                ))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            overlay,
        );
    }

    fn panel_block<'a>(title: impl Into<Line<'a>>, focused: bool, theme: ThemePreset) -> Block<'a> {
        let title = title.into();
        let title = if focused {
            Line::from(format!("{title} [focus]"))
        } else {
            title
        };
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.panel_style(focused))
            .title_style(theme.panel_style(focused))
            .title(title)
    }

    fn layout(
        area: Rect,
        inspector_open: bool,
        drawer_open: bool,
        preset: LayoutPreset,
    ) -> ShellRects {
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
        let show_navigation = preset != LayoutPreset::Focus
            && area.width >= 52
            && (!show_inspector || area.width >= 84);
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
                let timeout = if workspace.selected_view_has_background_work() {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs(30)
                };
                if !event::poll(timeout)? {
                    if workspace.selected_view_has_background_work() {
                        break;
                    }
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
        let layout = Workspace::layout(Rect::new(0, 0, 40, 10), true, true, LayoutPreset::Balanced);
        assert!(!layout.state.navigation);
        assert!(!layout.state.inspector);
        assert!(!layout.state.drawer);
    }

    #[test]
    fn layout_shows_optional_panels_when_space_allows() {
        let layout =
            Workspace::layout(Rect::new(0, 0, 120, 36), true, true, LayoutPreset::Balanced);
        assert!(layout.state.navigation);
        assert!(layout.state.inspector);
        assert!(layout.state.drawer);
    }

    #[test]
    fn inspector_uses_the_available_width_before_navigation() {
        let layout =
            Workspace::layout(Rect::new(0, 0, 60, 20), true, false, LayoutPreset::Balanced);
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
