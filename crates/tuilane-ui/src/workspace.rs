use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use tuilane_core::{DuplicateViewId, ViewRegistry};
use tuilane_plugin_api::{
    DashboardOption, DashboardOptionKind, DashboardOptionValue, DashboardSize, DashboardTone,
    DashboardWidget, DashboardWidgetContent, DashboardWidgetPreference, ViewId,
};

use crate::{LayoutPreset, ThemePreset, View, ViewDialog, WorkspaceConfig, icons};

const TAB_ROW_HEIGHT: u16 = 1;
const SIDE_PANEL_WIDTH: u16 = 34;
const MIN_MAIN_WIDTH: u16 = 36;
const NOTIFICATION_DURATION: Duration = Duration::from_secs(3);

/// The shell region currently receiving keyboard navigation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FocusArea {
    /// The top tab row. Moving here changes the active view immediately.
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
    tab_cursor: Option<usize>,
    tab_scroll: usize,
    focus: FocusArea,
    inspector_open: bool,
    drawer_open: bool,
    help_open: bool,
    palette_open: bool,
    switcher_open: bool,
    switcher_selected: usize,
    switcher_query: String,
    dashboard_editor_open: bool,
    dashboard_options_open: bool,
    dashboard_cursor: usize,
    dashboard_option_cursor: usize,
    dashboard_scroll: usize,
    dashboard_layout_dirty: bool,
    palette_query: String,
    palette_selected: usize,
    should_quit: bool,
    notification: Option<WorkspaceNotification>,
    config: WorkspaceConfig,
    last_layout: ShellLayout,
    last_regions: ShellRects,
    last_tab_items: Vec<(usize, Rect)>,
    last_switcher_items: Vec<(usize, Rect)>,
    last_dashboard_items: Vec<DashboardHit>,
    last_dashboard_editor_items: Vec<DashboardEditorHit>,
    last_dashboard_option_items: Vec<DashboardOptionHit>,
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
    context: String,
    description: String,
    icon: &'static str,
    search_text: String,
    action: PaletteAction,
}

struct WorkspaceNotification {
    message: String,
    expires_at: Instant,
}

struct DashboardEntry {
    owner_index: usize,
    owner_icon: &'static str,
    widget: DashboardWidget,
    size: DashboardSize,
}

struct DashboardHit {
    position: usize,
    owner_index: usize,
    widget_id: tuilane_plugin_api::DashboardWidgetId,
    area: Rect,
}

#[derive(Clone, Copy)]
struct DashboardEditorHit {
    index: usize,
    row: Rect,
    visibility: Rect,
    size: Rect,
    options: Option<Rect>,
}

#[derive(Clone, Copy)]
struct DashboardOptionHit {
    index: usize,
    area: Rect,
}

fn dashboard_span(size: DashboardSize, columns: u16) -> u16 {
    match size {
        DashboardSize::Compact => 1,
        DashboardSize::Wide => 2.min(columns),
        DashboardSize::Full => columns,
    }
}

fn dashboard_rows(entries: Vec<DashboardEntry>, columns: u16) -> Vec<Vec<DashboardEntry>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut used = 0_u16;

    for entry in entries {
        let span = dashboard_span(entry.size, columns);
        if !row.is_empty() && used.saturating_add(span) > columns {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        row.push(entry);
        used += span;
        if used >= columns {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn render_dashboard_widget(
    frame: &mut Frame<'_>,
    area: Rect,
    entry: &DashboardEntry,
    selected: bool,
    theme: ThemePreset,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let descriptor = entry.widget.descriptor();
    let accent = if selected {
        theme.accent_style().add_modifier(Modifier::BOLD)
    } else {
        theme.muted_style()
    };
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(entry.owner_icon, theme.accent_style()),
        Span::raw(" "),
        Span::styled(descriptor.title(), theme.header_style()),
        Span::raw(" "),
    ]);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(accent)
        .title(title);
    let content_area = block.inner(area);
    frame.render_widget(block, area);

    let lines = match entry.widget.content() {
        DashboardWidgetContent::Metric {
            value,
            label,
            detail,
            tone,
        } => {
            let mut lines = vec![
                Line::from(Span::styled(
                    value.as_ref(),
                    dashboard_tone_style(*tone, theme).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(label.as_ref(), theme.muted_style())),
            ];
            if let Some(detail) = detail {
                lines.push(Line::from(detail.as_ref()).style(theme.muted_style()));
            }
            lines
        }
        DashboardWidgetContent::List {
            items,
            empty_message,
        } => {
            if items.is_empty() {
                vec![Line::from(empty_message.as_ref()).style(theme.muted_style())]
            } else {
                items
                    .iter()
                    .take(usize::from(content_area.height.max(1)))
                    .map(|item| {
                        let mut spans = vec![
                            Span::styled("› ", theme.accent_style()),
                            Span::styled(item.title(), dashboard_tone_style(item.tone(), theme)),
                        ];
                        if let Some(detail) = item.detail() {
                            spans.push(Span::raw("  "));
                            spans.push(Span::styled(detail, theme.muted_style()));
                        }
                        Line::from(spans)
                    })
                    .collect()
            }
        }
        DashboardWidgetContent::Message {
            title,
            detail,
            tone,
        } => vec![
            Line::from(Span::styled(
                title.as_ref(),
                dashboard_tone_style(*tone, theme),
            )),
            Line::from(detail.as_ref()).style(theme.muted_style()),
        ],
    };
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        content_area,
    );
}

fn dashboard_tone_style(tone: DashboardTone, theme: ThemePreset) -> Style {
    match tone {
        DashboardTone::Default => Style::default(),
        DashboardTone::Muted => theme.muted_style(),
        DashboardTone::Accent => theme.accent_style(),
        DashboardTone::Positive => Style::default().fg(Color::Green),
        DashboardTone::Warning => Style::default().fg(Color::Yellow),
        DashboardTone::Critical => Style::default().fg(Color::Red),
    }
}

fn default_option_value(option: &DashboardOption) -> DashboardOptionValue {
    option.default_value()
}

fn option_value_matches(option: &DashboardOptionKind, value: &DashboardOptionValue) -> bool {
    option.accepts(value)
}

fn adjust_option_value(
    option: &DashboardOptionKind,
    value: DashboardOptionValue,
    forward: bool,
) -> Option<DashboardOptionValue> {
    match (option, value) {
        (DashboardOptionKind::Toggle { .. }, DashboardOptionValue::Toggle(value)) => {
            Some(DashboardOptionValue::Toggle(!value))
        }
        (DashboardOptionKind::Number { min, max, .. }, DashboardOptionValue::Number(value)) => {
            let upper = (*max).max(*min);
            let value = value.clamp(*min, upper);
            Some(DashboardOptionValue::Number(if forward {
                value.saturating_add(1).min(upper)
            } else {
                value.saturating_sub(1).max(*min)
            }))
        }
        (DashboardOptionKind::Choice { choices, default }, DashboardOptionValue::Choice(value))
            if !choices.is_empty() =>
        {
            let current = choices
                .iter()
                .position(|choice| choice.value() == value.as_ref())
                .or_else(|| {
                    choices
                        .iter()
                        .position(|choice| choice.value() == default.as_ref())
                })
                .unwrap_or(0);
            let next = if forward {
                (current + 1) % choices.len()
            } else {
                (current + choices.len() - 1) % choices.len()
            };
            Some(DashboardOptionValue::Choice(choices[next].value().into()))
        }
        (kind, _) => Some(default_option_value(&DashboardOption::new(
            "",
            "",
            kind.clone(),
        ))),
    }
}

fn option_value_label(value: DashboardOptionValue) -> String {
    match value {
        DashboardOptionValue::Toggle(true) => "On".to_owned(),
        DashboardOptionValue::Toggle(false) => "Off".to_owned(),
        DashboardOptionValue::Number(value) => value.to_string(),
        DashboardOptionValue::Choice(value) => value.into(),
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ShellLayout {
    inspector: bool,
    drawer: bool,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            views: ViewRegistry::new(),
            selected: None,
            tab_cursor: None,
            tab_scroll: 0,
            focus: FocusArea::Content,
            inspector_open: false,
            drawer_open: false,
            help_open: false,
            palette_open: false,
            switcher_open: false,
            switcher_selected: 0,
            switcher_query: String::new(),
            dashboard_editor_open: false,
            dashboard_options_open: false,
            dashboard_cursor: 0,
            dashboard_option_cursor: 0,
            dashboard_scroll: 0,
            dashboard_layout_dirty: false,
            palette_query: String::new(),
            palette_selected: 0,
            should_quit: false,
            notification: None,
            config: WorkspaceConfig::default(),
            last_layout: ShellLayout::default(),
            last_regions: ShellRects::default(),
            last_tab_items: Vec::new(),
            last_switcher_items: Vec::new(),
            last_dashboard_items: Vec::new(),
            last_dashboard_editor_items: Vec::new(),
            last_dashboard_option_items: Vec::new(),
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
            self.tab_cursor = Some(0);
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
        self.tab_cursor = match (self.tab_cursor, self.views.is_empty()) {
            (_, true) => None,
            (Some(cursor), false) if cursor > removed_index => Some(cursor - 1),
            (Some(cursor), false) if cursor == removed_index => {
                Some(cursor.min(self.views.len() - 1))
            }
            (cursor, false) => cursor,
        };
        if self.views.is_empty() {
            self.focus = FocusArea::Content;
            self.inspector_open = false;
            self.drawer_open = false;
        }
        self.tab_scroll = self.tab_scroll.min(self.views.len().saturating_sub(1));
        self.switcher_selected = self
            .switcher_selected
            .min(self.views.len().saturating_sub(1));
        self.synchronize_switcher_selection();

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
        if self.inspector_open {
            self.inspector_open = false;
            if self.focus == FocusArea::Inspector {
                self.focus = FocusArea::Content;
            }
        } else {
            self.drawer_open = false;
            self.inspector_open = true;
            self.focus = FocusArea::Inspector;
        }
    }

    /// Opens or closes the supplemental side panel.
    pub fn toggle_drawer(&mut self) {
        if self.drawer_open {
            self.drawer_open = false;
            if self.focus == FocusArea::Drawer {
                self.focus = FocusArea::Content;
            }
        } else {
            self.inspector_open = false;
            self.drawer_open = true;
            self.focus = FocusArea::Drawer;
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

    /// Returns the current shell and dashboard configuration.
    pub fn config(&self) -> &WorkspaceConfig {
        &self.config
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
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }

        if self.palette_open {
            self.handle_palette_key(key);
            self.capture_notification();
            return;
        }

        if self.switcher_open {
            self.handle_switcher_key(key);
            self.capture_notification();
            return;
        }

        if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.toggle_command_palette();
            return;
        }

        if is_alt_tab_key(key) {
            let backwards =
                key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
            self.open_tab_switcher(backwards);
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

        if self.dashboard_editor_open {
            self.handle_dashboard_editor_key(key);
            return;
        }

        let view_consumed = self
            .selected
            .and_then(|index| self.views.get_mut(index))
            .is_some_and(|view| view.value_mut().handle_key(key, self.focus));
        self.capture_notification();
        if view_consumed {
            return;
        }

        if self.is_dashboard_active() && self.focus == FocusArea::Content {
            match key.code {
                KeyCode::Char('e') if key.modifiers.is_empty() => {
                    self.open_dashboard_editor();
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') if key.modifiers.is_empty() => {
                    self.move_dashboard_selection(false);
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') if key.modifiers.is_empty() => {
                    self.move_dashboard_selection(true);
                    return;
                }
                KeyCode::Enter if key.modifiers.is_empty() => {
                    self.activate_dashboard_selection();
                    return;
                }
                _ => {}
            }
        }

        if self.focus == FocusArea::Content
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            let list_key = match key.code {
                KeyCode::Char('j') => Some(KeyCode::Down),
                KeyCode::Char('k') => Some(KeyCode::Up),
                _ => None,
            };
            if let Some(code) = list_key {
                let alias = KeyEvent { code, ..key };
                if self
                    .selected
                    .and_then(|index| self.views.get_mut(index))
                    .is_some_and(|view| view.value_mut().handle_key(alias, FocusArea::Content))
                {
                    return;
                }
            }
        }

        if is_shift_view_switch_key(key) {
            let forward = matches!(key.code, KeyCode::Char('l' | 'L'));
            if self.focus == FocusArea::Navigation {
                self.move_tab_cursor(forward);
            } else {
                self.switch_adjacent_view(forward);
            }
            return;
        }

        if self.focus == FocusArea::Navigation {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.move_tab_cursor(true);
                    return;
                }
                KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('h') => {
                    self.move_tab_cursor(false);
                    return;
                }
                KeyCode::Char('l') | KeyCode::Left | KeyCode::Enter => {
                    self.activate_tab_cursor();
                    return;
                }
                KeyCode::Right => {
                    self.move_tab_cursor(true);
                    return;
                }
                KeyCode::Esc => {
                    self.cancel_tab_navigation();
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.help_open = true,
            KeyCode::Esc => self.focus = FocusArea::Content,
            KeyCode::Char('i') => self.toggle_inspector(),
            KeyCode::Char('d') => self.toggle_drawer(),
            _ => {}
        }
    }

    /// Routes mouse input to visible workspace panels.
    ///
    /// Clicking or scrolling over a tab activates it. Clicking another visible
    /// panel moves keyboard focus to that panel.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        if self.dashboard_editor_open {
            return self.handle_dashboard_editor_mouse(mouse);
        }
        if self.help_open || self.palette_open {
            return false;
        }

        if self.switcher_open {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(index) = self.switcher_index_at(mouse.column, mouse.row) {
                        self.activate_view(index);
                    }
                    self.switcher_open = false;
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                    if rect_contains(self.last_regions.switcher, mouse.column, mouse.row) =>
                {
                    self.move_switcher_selection(mouse.kind == MouseEventKind::ScrollDown);
                }
                _ => {}
            }
            return true;
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.is_dashboard_active()
                    && let Some((owner_index, widget_id)) = self
                        .last_dashboard_items
                        .iter()
                        .find(|hit| rect_contains(hit.area, mouse.column, mouse.row))
                        .map(|hit| (hit.owner_index, hit.widget_id.clone()))
                {
                    self.activate_dashboard_view(owner_index, &widget_id);
                    return true;
                }
                if self
                    .selected
                    .and_then(|index| self.views.get(index))
                    .is_some_and(|view| view.value().dialog().is_some())
                {
                    return true;
                }
                let focus_changed = self.focus_at(mouse.column, mouse.row);
                let view_handled = if rect_contains(self.last_regions.main, mouse.column, mouse.row)
                    && self.focus == FocusArea::Content
                {
                    self.selected
                        .and_then(|index| self.views.get_mut(index))
                        .is_some_and(|view| view.value_mut().handle_mouse(mouse, self.focus))
                } else {
                    false
                };
                focus_changed || view_handled
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if !rect_contains(self.last_regions.tabs, mouse.column, mouse.row) {
                    return false;
                }

                let previous_focus = self.focus;
                let previous_selection = self.selected;
                self.move_tab_cursor(mouse.kind == MouseEventKind::ScrollDown);
                self.focus = FocusArea::Content;
                previous_focus != self.focus || previous_selection != self.selected
            }
            _ => false,
        }
    }

    fn handle_dashboard_editor_mouse(&mut self, mouse: MouseEvent) -> bool {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return true;
        }

        if self.dashboard_options_open {
            if let Some(hit) = self
                .last_dashboard_option_items
                .iter()
                .find(|hit| rect_contains(hit.area, mouse.column, mouse.row))
            {
                let hit = *hit;
                let forward =
                    mouse.column >= hit.area.x.saturating_add(hit.area.width.saturating_div(2));
                self.dashboard_option_cursor = hit.index;
                self.adjust_dashboard_option(forward);
            }
            return true;
        }

        if let Some(hit) = self
            .last_dashboard_editor_items
            .iter()
            .find(|hit| rect_contains(hit.row, mouse.column, mouse.row))
            .copied()
        {
            self.dashboard_cursor = hit.index;
            if rect_contains(hit.visibility, mouse.column, mouse.row) {
                self.toggle_dashboard_widget();
            } else if rect_contains(hit.size, mouse.column, mouse.row) {
                self.resize_dashboard_widget(true);
            } else if hit
                .options
                .is_some_and(|area| rect_contains(area, mouse.column, mouse.row))
            {
                self.open_dashboard_options();
            }
        }
        true
    }

    fn focus_at(&mut self, column: u16, row: u16) -> bool {
        let regions = self.last_regions;
        if rect_contains(regions.tabs, column, row) {
            let previous_selection = self.selected;
            let previous_focus = self.focus;
            if let Some(index) = self.tab_index_at(column) {
                self.activate_view(index);
            } else {
                self.tab_cursor = self.selected;
                self.focus = FocusArea::Navigation;
            }
            return previous_selection != self.selected || previous_focus != self.focus;
        }

        let target = if rect_contains(regions.main, column, row) {
            Some(FocusArea::Content)
        } else if regions
            .inspector
            .is_some_and(|area| rect_contains(area, column, row))
        {
            Some(FocusArea::Inspector)
        } else if regions
            .drawer
            .is_some_and(|area| rect_contains(area, column, row))
        {
            Some(FocusArea::Drawer)
        } else {
            None
        };

        if let Some(target) = target {
            let changed = self.focus != target;
            self.focus = target;
            changed
        } else {
            false
        }
    }

    fn open_tab_switcher(&mut self, backwards: bool) {
        if self.views.is_empty() {
            return;
        }
        self.switcher_query.clear();
        self.switcher_selected = self.selected.unwrap_or(0);
        self.switcher_open = true;
        self.move_switcher_selection(!backwards);
    }

    fn activate_view(&mut self, index: usize) {
        if index >= self.views.len() {
            return;
        }
        self.selected = Some(index);
        self.tab_cursor = Some(index);
        self.focus = FocusArea::Content;
        self.ensure_tab_visible(self.last_regions.tabs.width);
    }

    fn switch_adjacent_view(&mut self, forward: bool) {
        let Some(current) = self.selected else {
            return;
        };
        let next = if forward {
            current
                .saturating_add(1)
                .min(self.views.len().saturating_sub(1))
        } else {
            current.saturating_sub(1)
        };
        self.activate_view(next);
    }

    fn handle_switcher_key(&mut self, key: KeyEvent) {
        if is_alt_tab_key(key) {
            self.move_switcher_selection(
                key.code != KeyCode::BackTab && !key.modifiers.contains(KeyModifiers::SHIFT),
            );
            return;
        }

        match key.code {
            KeyCode::Esc => self.switcher_open = false,
            KeyCode::Enter => {
                if self.switcher_candidates().contains(&self.switcher_selected) {
                    self.activate_view(self.switcher_selected);
                    self.switcher_open = false;
                }
            }
            KeyCode::Up | KeyCode::Left => {
                self.move_switcher_selection(false);
            }
            KeyCode::Down | KeyCode::Right => {
                self.move_switcher_selection(true);
            }
            KeyCode::Backspace => {
                self.switcher_query.pop();
                self.synchronize_switcher_selection();
            }
            KeyCode::Char(character)
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                self.switcher_query.push(character);
                self.synchronize_switcher_selection();
            }
            _ => {}
        }
    }

    fn move_switcher_selection(&mut self, forward: bool) {
        let candidates = self.switcher_candidates();
        if candidates.is_empty() {
            return;
        }
        let current = candidates
            .iter()
            .position(|index| *index == self.switcher_selected)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % candidates.len()
        } else {
            (current + candidates.len() - 1) % candidates.len()
        };
        self.switcher_selected = candidates[next];
    }

    fn switcher_candidates(&self) -> Vec<usize> {
        let query = self.switcher_query.to_lowercase();
        self.views
            .iter()
            .enumerate()
            .filter(|(_, view)| {
                query.is_empty()
                    || view.descriptor().title().to_lowercase().contains(&query)
                    || view
                        .descriptor()
                        .id()
                        .as_str()
                        .to_lowercase()
                        .contains(&query)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn synchronize_switcher_selection(&mut self) {
        let candidates = self.switcher_candidates();
        if !candidates.contains(&self.switcher_selected)
            && let Some(first) = candidates.first()
        {
            self.switcher_selected = *first;
        }
    }

    fn switcher_index_at(&self, column: u16, row: u16) -> Option<usize> {
        self.last_switcher_items
            .iter()
            .find(|(_, item)| rect_contains(*item, column, row))
            .map(|(index, _)| *index)
    }

    fn render_tab_switcher(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.last_switcher_items.clear();
        if area.width < 12 || area.height < 6 || self.views.is_empty() {
            self.last_regions.switcher = Rect::default();
            return;
        }

        let candidates = self.switcher_candidates();
        let width = area.width.saturating_sub(4).min(60);
        let inner_height = usize::from(area.height.saturating_sub(4));
        let show_detail = inner_height >= 3 && !candidates.is_empty();
        let show_hints = inner_height >= 4;
        let visible_count = if candidates.is_empty() {
            0
        } else {
            candidates.len().min(
                inner_height
                    .saturating_sub(1 + usize::from(show_detail) + usize::from(show_hints))
                    .max(1),
            )
        };
        let line_count = 1
            + visible_count.max(usize::from(candidates.is_empty()))
            + usize::from(show_detail)
            + usize::from(show_hints);
        let height = (line_count as u16 + 2).min(area.height.saturating_sub(2));
        let overlay = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        };
        self.last_regions.switcher = overlay;

        let theme = self.config.theme();
        let inner_width = usize::from(width.saturating_sub(2));
        let mut lines = vec![search_prompt_line(
            " / ",
            &self.switcher_query,
            &format!("{}/{}", candidates.len(), self.views.len()),
            inner_width,
            theme,
        )];
        if candidates.is_empty() {
            lines.push(Line::from("No matching views").style(theme.muted_style()));
        } else {
            let selected_position = candidates
                .iter()
                .position(|index| *index == self.switcher_selected)
                .unwrap_or(0);
            let start = selected_position.saturating_sub(visible_count.saturating_sub(1));
            for (offset, index) in candidates
                .iter()
                .copied()
                .skip(start)
                .take(visible_count)
                .enumerate()
            {
                let Some(view) = self.views.get(index) else {
                    continue;
                };
                let selected = index == self.switcher_selected;
                let marker = if selected { "›" } else { " " };
                let label = truncate_to_width(
                    &format!(
                        "{marker}  {}  {}",
                        view.value().icon(),
                        view.descriptor().title()
                    ),
                    inner_width,
                );
                let item_area =
                    Rect::new(overlay.x + 1, overlay.y + 2 + offset as u16, width - 2, 1);
                self.last_switcher_items.push((index, item_area));
                let style = if selected {
                    theme.navigation_item_style(true)
                } else {
                    theme.muted_style()
                };
                lines.push(Line::from(label).style(style));
            }
            if show_detail && let Some(view) = self.views.get(self.switcher_selected) {
                lines.push(
                    Line::from(truncate_to_width(
                        view.descriptor().description(),
                        inner_width,
                    ))
                    .style(theme.muted_style()),
                );
            }
        }
        if show_hints {
            lines.push(popup_hints_line(
                theme,
                &[("↑/↓", "Move"), ("Enter", "Open"), ("Esc", "Close")],
            ));
        }

        frame.render_widget(Clear, overlay);
        frame.render_widget(
            Paragraph::new(Text::from(lines)).block(Self::panel_block("Views", true, theme)),
            overlay,
        );
    }

    fn tab_index_at(&self, column: u16) -> Option<usize> {
        self.last_tab_items
            .iter()
            .find(|(_, area)| rect_contains(*area, column, self.last_regions.tabs.y))
            .map(|(index, _)| *index)
    }

    /// Draws the responsive workspace shell and its registered contributions.
    pub fn render(&mut self, frame: &mut Frame<'_>) {
        self.expire_notification();
        let area = frame.area();
        let footer = self.footer_line(usize::from(area.width));
        let layout = Self::layout(
            area,
            self.inspector_open,
            self.drawer_open,
            self.config.layout(),
            1,
        );
        self.last_layout = layout.state;
        self.last_regions = layout;
        self.normalize_focus(layout.state);

        self.ensure_tab_visible(layout.tabs.width);
        self.render_tabs(frame, layout.tabs);
        self.render_content(frame, layout.main);
        self.capture_notification();
        if let Some(inspector) = layout.inspector {
            self.render_inspector(frame, inspector);
        }
        if let Some(drawer) = layout.drawer {
            self.render_drawer(frame, drawer);
        }
        frame.render_widget(Paragraph::new(footer), layout.status);

        if !self.help_open && !self.palette_open && !self.switcher_open {
            if self.render_view_dialog(frame, area) {
                return;
            }
            self.render_notification(frame, area);
        }

        if self.help_open {
            self.render_help(frame, area);
        }
        if self.palette_open {
            self.render_command_palette(frame, area);
        }
        if self.switcher_open {
            self.render_tab_switcher(frame, area);
        }
        if self.dashboard_editor_open {
            self.render_dashboard_editor(frame, area);
        }
    }

    fn render_tabs(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.last_tab_items.clear();
        if area.width == 0 || area.height == 0 {
            return;
        }

        let anchor = if self.focus == FocusArea::Navigation {
            self.tab_cursor
        } else {
            self.selected
        };
        if anchor.is_none() {
            return;
        }

        let mut x = area.x;
        for (index, view) in self.views.iter().enumerate().skip(self.tab_scroll) {
            if x >= area.x.saturating_add(area.width) {
                break;
            }
            let remaining = area.x.saturating_add(area.width).saturating_sub(x);
            let style = if Some(index) == self.selected {
                self.config.theme().navigation_item_style(true)
            } else if self.focus == FocusArea::Navigation && Some(index) == self.tab_cursor {
                self.config
                    .theme()
                    .header_style()
                    .add_modifier(ratatui::style::Modifier::UNDERLINED)
            } else {
                self.config.theme().muted_style()
            };
            let icon_style = if Some(index) == self.selected {
                style
            } else {
                self.config.theme().accent_style()
            };
            let Some(label) = tab_label(
                view.value().icon(),
                view.descriptor().title(),
                usize::from(remaining),
                style,
                icon_style,
            ) else {
                break;
            };
            let width = label.width().min(usize::from(remaining)) as u16;
            if width == 0 {
                break;
            }
            let item_area = Rect::new(x, area.y, width, 1);
            frame.render_widget(Paragraph::new(label).style(style), item_area);
            self.last_tab_items.push((index, item_area));
            x = x.saturating_add(width).saturating_add(1);
        }
    }

    fn render_content(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let Some(index) = self.selected else {
            let block = Self::panel_block(
                Line::default(),
                self.focus == FocusArea::Content,
                self.config.theme(),
            );
            let inner = block.inner(area);
            frame.render_widget(block, area);
            crate::render_empty_state(
                frame,
                inner,
                "No views available",
                "Register a view to begin.",
            );
            return;
        };
        let is_dashboard = self
            .views
            .get(index)
            .is_some_and(|view| view.value().is_dashboard());
        if self.views.get(index).is_none() {
            return;
        }
        let block = Self::panel_block(
            Line::default(),
            self.focus == FocusArea::Content,
            self.config.theme(),
        );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if is_dashboard {
            self.render_dashboard(frame, inner);
        } else {
            if let Some(view) = self.views.get_mut(index) {
                view.value_mut()
                    .render_with_theme(frame, inner, self.config.theme());
            }
        }
    }

    fn is_dashboard_active(&self) -> bool {
        self.selected
            .and_then(|index| self.views.get(index))
            .is_some_and(|view| view.value().is_dashboard())
    }

    fn collect_dashboard_entries(&mut self, include_hidden: bool) -> Vec<DashboardEntry> {
        let layout = self.config.dashboard_layout().clone();
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for owner_index in 0..self.views.len() {
            let Some(registered) = self.views.get_mut(owner_index) else {
                continue;
            };
            let owner_icon = registered.value().icon();
            let widgets = registered.value_mut().dashboard_widgets(&layout);
            for widget in widgets {
                let id = widget.descriptor().id();
                if !seen.insert(id.clone()) {
                    continue;
                }
                let preference = layout.preference(id);
                if !include_hidden && preference.is_some_and(|value| !value.visible()) {
                    continue;
                }
                let size =
                    preference.map_or(widget.descriptor().default_size(), |value| value.size());
                entries.push(DashboardEntry {
                    owner_index,
                    owner_icon,
                    widget,
                    size,
                });
            }
        }

        entries.sort_by_key(|entry| {
            layout
                .widgets()
                .iter()
                .position(|saved| saved.id() == entry.widget.descriptor().id())
                .unwrap_or(usize::MAX)
        });
        entries
    }

    fn render_dashboard(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.last_dashboard_items.clear();
        let mut entries = self.collect_dashboard_entries(false);
        if entries.is_empty() {
            crate::render_empty_state(
                frame,
                area,
                "Your overview is empty",
                "Press e to add widgets from the available views.",
            );
            return;
        }

        let columns = match area.width {
            0..=54 => 1,
            55..=94 => 2,
            _ => 3,
        };
        let rows = dashboard_rows(std::mem::take(&mut entries), columns);
        self.dashboard_cursor = self
            .dashboard_cursor
            .min(rows.iter().map(Vec::len).sum::<usize>().saturating_sub(1));

        let row_capacity = usize::from(area.height.saturating_add(1) / 7).max(1);
        let mut row_start = 0;
        let selected_row = rows
            .iter()
            .position(|row| {
                let contains_selection = self.dashboard_cursor < row_start + row.len();
                row_start += row.len();
                contains_selection
            })
            .unwrap_or(0);
        if selected_row < self.dashboard_scroll {
            self.dashboard_scroll = selected_row;
        } else if selected_row >= self.dashboard_scroll.saturating_add(row_capacity) {
            self.dashboard_scroll = selected_row.saturating_add(1).saturating_sub(row_capacity);
        }
        self.dashboard_scroll = self
            .dashboard_scroll
            .min(rows.len().saturating_sub(row_capacity));

        let visible_rows =
            &rows[self.dashboard_scroll..rows.len().min(self.dashboard_scroll + row_capacity)];
        let row_areas = Layout::vertical(vec![Constraint::Min(6); visible_rows.len()])
            .spacing(1)
            .split(area);
        let theme = self.config.theme();
        let mut absolute_index = rows
            .iter()
            .take(self.dashboard_scroll)
            .map(Vec::len)
            .sum::<usize>();
        for (row, row_area) in visible_rows.iter().zip(row_areas.iter()) {
            let column_areas = Layout::horizontal(
                (0..columns)
                    .map(|_| Constraint::Ratio(1, u32::from(columns)))
                    .collect::<Vec<_>>(),
            )
            .spacing(1)
            .split(*row_area);
            let mut column = 0_usize;
            for entry in row {
                let span = usize::from(dashboard_span(entry.size, columns));
                let Some(first) = column_areas.get(column) else {
                    break;
                };
                let Some(last) = column_areas.get(column + span - 1) else {
                    break;
                };
                let cell = Rect::new(
                    first.x,
                    row_area.y,
                    last.x.saturating_add(last.width).saturating_sub(first.x),
                    row_area.height,
                );
                let selected = absolute_index == self.dashboard_cursor;
                render_dashboard_widget(frame, cell, entry, selected, theme);
                self.last_dashboard_items.push(DashboardHit {
                    position: absolute_index,
                    owner_index: entry.owner_index,
                    widget_id: entry.widget.descriptor().id().clone(),
                    area: cell,
                });
                absolute_index += 1;
                column += span;
            }
        }
    }

    fn move_dashboard_selection(&mut self, forward: bool) {
        let entries = self.collect_dashboard_entries(false);
        if entries.is_empty() {
            self.dashboard_cursor = 0;
            return;
        }
        self.dashboard_cursor = if forward {
            (self.dashboard_cursor + 1) % entries.len()
        } else {
            (self.dashboard_cursor + entries.len() - 1) % entries.len()
        };
    }

    fn activate_dashboard_selection(&mut self) {
        if let Some((owner_index, widget_id)) = self
            .last_dashboard_items
            .iter()
            .find(|hit| hit.position == self.dashboard_cursor)
            .map(|hit| (hit.owner_index, hit.widget_id.clone()))
        {
            self.activate_dashboard_view(owner_index, &widget_id);
        }
    }

    fn activate_dashboard_view(
        &mut self,
        index: usize,
        widget_id: &tuilane_plugin_api::DashboardWidgetId,
    ) {
        if index < self.views.len() {
            if let Some(view) = self.views.get_mut(index) {
                view.value_mut().open_dashboard_widget(widget_id);
            }
            self.selected = Some(index);
            self.tab_cursor = Some(index);
            self.focus = FocusArea::Content;
            self.dashboard_scroll = 0;
        }
    }

    fn open_dashboard_editor(&mut self) {
        if !self.is_dashboard_active() || self.focus != FocusArea::Content {
            return;
        }
        let entries = self.collect_dashboard_entries(true);
        let layout = self.config.dashboard_layout_mut();
        for entry in &entries {
            let descriptor = entry.widget.descriptor();
            if layout.preference(descriptor.id()).is_none() {
                layout.set_preference(DashboardWidgetPreference::new(
                    descriptor.id().clone(),
                    descriptor.default_size(),
                ));
            }
            if let Some(preference) = layout.preference_mut(descriptor.id()) {
                for option in descriptor.options() {
                    let valid = preference
                        .option(option.id())
                        .is_some_and(|value| option_value_matches(option.kind(), value));
                    if !valid {
                        preference.set_option(option.id(), default_option_value(option));
                    }
                }
            }
        }
        self.dashboard_editor_open = true;
        self.dashboard_options_open = false;
        self.dashboard_cursor = 0;
        self.dashboard_option_cursor = 0;
        self.dashboard_layout_dirty = false;
    }

    fn handle_dashboard_editor_key(&mut self, key: KeyEvent) {
        if self.dashboard_options_open {
            self.handle_dashboard_options_key(key);
            return;
        }

        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.reorder_dashboard_widget(-1),
                KeyCode::Down | KeyCode::Char('j') => self.reorder_dashboard_widget(1),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_dashboard_editor(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_editor_selection(false);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_editor_selection(true);
            }
            KeyCode::Char(' ') => self.toggle_dashboard_widget(),
            KeyCode::Char('o') => self.open_dashboard_options(),
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right => {
                self.resize_dashboard_widget(true);
            }
            KeyCode::Char('-') | KeyCode::Left => self.resize_dashboard_widget(false),
            _ => {}
        }
    }

    fn move_editor_selection(&mut self, forward: bool) {
        let entries = self.collect_dashboard_entries(true);
        if entries.is_empty() {
            self.dashboard_cursor = 0;
        } else if forward {
            self.dashboard_cursor = (self.dashboard_cursor + 1) % entries.len();
        } else {
            self.dashboard_cursor = (self.dashboard_cursor + entries.len() - 1) % entries.len();
        }
        self.dashboard_option_cursor = 0;
    }

    fn current_editor_widget(&mut self) -> Option<DashboardEntry> {
        self.collect_dashboard_entries(true)
            .into_iter()
            .nth(self.dashboard_cursor)
    }

    fn toggle_dashboard_widget(&mut self) {
        let Some(entry) = self.current_editor_widget() else {
            return;
        };
        let id = entry.widget.descriptor().id();
        if let Some(preference) = self.config.dashboard_layout_mut().preference_mut(id) {
            preference.set_visible(!preference.visible());
            self.dashboard_layout_dirty = true;
        }
    }

    fn resize_dashboard_widget(&mut self, wider: bool) {
        let Some(entry) = self.current_editor_widget() else {
            return;
        };
        let id = entry.widget.descriptor().id();
        if let Some(preference) = self.config.dashboard_layout_mut().preference_mut(id) {
            let next = match (preference.size(), wider) {
                (DashboardSize::Compact, true) => DashboardSize::Wide,
                (DashboardSize::Wide, true) => DashboardSize::Full,
                (DashboardSize::Full, true) => DashboardSize::Compact,
                (DashboardSize::Full, false) => DashboardSize::Wide,
                (DashboardSize::Wide, false) => DashboardSize::Compact,
                (DashboardSize::Compact, false) => DashboardSize::Full,
            };
            preference.set_size(next);
            self.dashboard_layout_dirty = true;
        }
    }

    fn reorder_dashboard_widget(&mut self, direction: isize) {
        let Some(entry) = self.current_editor_widget() else {
            return;
        };
        if self
            .config
            .dashboard_layout_mut()
            .move_widget(entry.widget.descriptor().id(), direction)
        {
            self.dashboard_cursor = self.dashboard_cursor.saturating_add_signed(direction);
            self.dashboard_layout_dirty = true;
        }
    }

    fn open_dashboard_options(&mut self) {
        let Some(entry) = self.current_editor_widget() else {
            return;
        };
        if entry.widget.descriptor().options().is_empty() {
            return;
        }
        self.dashboard_options_open = true;
        self.dashboard_option_cursor = 0;
    }

    fn handle_dashboard_options_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.dashboard_options_open = false;
            return;
        }
        let Some(entry) = self.current_editor_widget() else {
            self.dashboard_options_open = false;
            return;
        };
        let options = entry.widget.descriptor().options();
        if options.is_empty() {
            self.dashboard_options_open = false;
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.dashboard_option_cursor =
                    (self.dashboard_option_cursor + options.len() - 1) % options.len();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.dashboard_option_cursor = (self.dashboard_option_cursor + 1) % options.len();
            }
            KeyCode::Left => self.adjust_dashboard_option(false),
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                self.adjust_dashboard_option(true);
            }
            _ => {}
        }
    }

    fn adjust_dashboard_option(&mut self, forward: bool) {
        let Some(entry) = self.current_editor_widget() else {
            return;
        };
        let Some(option) = entry
            .widget
            .descriptor()
            .options()
            .get(self.dashboard_option_cursor)
            .cloned()
        else {
            return;
        };
        let id = entry.widget.descriptor().id();
        let Some(preference) = self.config.dashboard_layout_mut().preference_mut(id) else {
            return;
        };
        let previous = preference
            .option(option.id())
            .cloned()
            .unwrap_or_else(|| default_option_value(&option));
        if let Some(next) = adjust_option_value(option.kind(), previous, forward) {
            preference.set_option(option.id(), next);
            self.dashboard_layout_dirty = true;
        }
    }

    fn close_dashboard_editor(&mut self) {
        self.dashboard_editor_open = false;
        self.dashboard_options_open = false;
        if self.dashboard_layout_dirty {
            self.notification = Some(WorkspaceNotification {
                message: "Overview updated".to_owned(),
                expires_at: Instant::now() + NOTIFICATION_DURATION,
            });
        }
        self.dashboard_layout_dirty = false;
    }

    fn render_dashboard_editor(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.last_dashboard_editor_items.clear();
        self.last_dashboard_option_items.clear();
        let mut controls = Vec::new();
        let entries = self.collect_dashboard_entries(true);
        let width = area.width.saturating_sub(4).min(78);
        let height = area.height.saturating_sub(4).min(24);
        if width < 24 || height < 7 {
            return;
        }
        let overlay = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        };
        let theme = self.config.theme();
        let title = if self.dashboard_options_open {
            entries
                .get(self.dashboard_cursor)
                .map_or("Widget options", |entry| entry.widget.descriptor().title())
        } else {
            "Customize overview"
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.accent_style())
            .title(Line::from(format!(" {title} ")).style(theme.header_style()));
        let inner = block.inner(overlay);
        frame.render_widget(Clear, overlay);
        frame.render_widget(block, overlay);

        let mut lines = Vec::new();
        if self.dashboard_options_open {
            let options = entries
                .get(self.dashboard_cursor)
                .map(|entry| entry.widget.descriptor().options())
                .unwrap_or_default();
            if options.is_empty() {
                lines.push(Line::from("This widget has no options."));
            } else {
                for (index, option) in options.iter().enumerate() {
                    let id = entries[self.dashboard_cursor].widget.descriptor().id();
                    let value = self
                        .config
                        .dashboard_layout()
                        .preference(id)
                        .and_then(|preference| preference.option(option.id()))
                        .cloned()
                        .unwrap_or_else(|| default_option_value(option));
                    let selected = index == self.dashboard_option_cursor;
                    let marker = if selected { "›" } else { " " };
                    let item_area = Rect::new(
                        inner.x,
                        inner.y.saturating_add(lines.len() as u16),
                        inner.width,
                        1,
                    );
                    self.last_dashboard_option_items.push(DashboardOptionHit {
                        index,
                        area: item_area,
                    });
                    let style = if selected {
                        theme.navigation_item_style(true)
                    } else {
                        Style::default()
                    };
                    lines.push(
                        Line::from(format!(
                            "{marker}  {}  {}",
                            option.label(),
                            option_value_label(value)
                        ))
                        .style(style),
                    );
                }
            }
            lines.push(Line::from("↑/↓ Choose   ←/→ Change   Esc Back").style(theme.muted_style()));
        } else if entries.is_empty() {
            lines.push(Line::from(
                "No views currently contribute dashboard widgets.",
            ));
            lines.push(Line::from(
                "Widgets will appear here as features are enabled.",
            ));
        } else {
            let visible_rows = usize::from(inner.height.saturating_sub(2) / 2).max(1);
            let start = self
                .dashboard_cursor
                .saturating_sub(visible_rows.saturating_sub(1));
            for (index, entry) in entries.iter().enumerate().skip(start).take(visible_rows) {
                let id = entry.widget.descriptor().id();
                let preference = self.config.dashboard_layout().preference(id);
                let enabled = preference.is_some_and(|value| value.visible());
                let size = preference.map_or(entry.widget.descriptor().default_size(), |value| {
                    value.size()
                });
                let selected = index == self.dashboard_cursor;
                let style = if selected {
                    theme.navigation_item_style(true)
                } else if enabled {
                    theme.panel_style(false)
                } else {
                    theme.muted_style()
                };
                let mark = if enabled { "✓" } else { "○" };
                let width = match size {
                    DashboardSize::Compact => "compact",
                    DashboardSize::Wide => "wide",
                    DashboardSize::Full => "full",
                };
                let row_area = Rect::new(
                    inner.x,
                    inner.y.saturating_add(lines.len() as u16),
                    inner.width,
                    1,
                );
                let has_options = !entry.widget.descriptor().options().is_empty();
                let label_width = 12.min(inner.width);
                let options_width = if has_options {
                    10.min(inner.width.saturating_sub(label_width))
                } else {
                    0
                };
                let size_width = 10.min(inner.width.saturating_sub(label_width + options_width));
                let text_width = inner
                    .width
                    .saturating_sub(options_width.saturating_add(size_width));
                let label_area = Rect::new(inner.x, row_area.y, text_width, 1);
                let size_area = Rect::new(
                    inner
                        .x
                        .saturating_add(inner.width.saturating_sub(size_width)),
                    row_area.y,
                    size_width,
                    1,
                );
                let options_area = (has_options && options_width > 0).then(|| {
                    Rect::new(
                        size_area.x.saturating_sub(options_width),
                        row_area.y,
                        options_width,
                        1,
                    )
                });
                self.last_dashboard_editor_items.push(DashboardEditorHit {
                    index,
                    row: row_area,
                    visibility: Rect::new(inner.x, row_area.y, 5.min(inner.width), 1),
                    size: size_area,
                    options: options_area,
                });
                controls.push((
                    label_area,
                    format!(
                        "{} {}  {}",
                        if selected { "›" } else { " " },
                        mark,
                        entry.widget.descriptor().title(),
                    ),
                    Alignment::Left,
                    style,
                ));
                if let Some(options_area) = options_area {
                    controls.push((
                        options_area,
                        "[Options]".to_owned(),
                        Alignment::Right,
                        style,
                    ));
                }
                controls.push((size_area, format!("[{width}]"), Alignment::Right, style));
                lines.push(Line::default());
                lines.push(
                    Line::from(format!(
                        "  {}  {}",
                        entry.owner_icon,
                        entry.widget.descriptor().description()
                    ))
                    .style(theme.muted_style()),
                );
            }
            lines.push(
                Line::from("Space Show/hide   Alt+↑/↓ Order   +/- Size   o Options   Esc Done")
                    .style(theme.muted_style()),
            );
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
        for (area, text, alignment, style) in controls {
            frame.render_widget(Paragraph::new(text).alignment(alignment).style(style), area);
        }
    }

    fn render_view_dialog(&self, frame: &mut Frame<'_>, area: Rect) -> bool {
        let Some(dialog) = self
            .selected
            .and_then(|index| self.views.get(index))
            .and_then(|view| view.value().dialog())
        else {
            return false;
        };
        let keybindings = self
            .selected
            .and_then(|index| self.views.get(index))
            .map(|view| view.value().keybindings(self.focus))
            .unwrap_or_default();

        let theme = self.config.theme();
        let width = area.width.saturating_sub(2).min(72);
        if width < 10 || area.height < 3 {
            return true;
        }

        let (title, mut lines, height) = match dialog {
            ViewDialog::Input {
                title,
                label,
                value,
                error,
            } => {
                let mut lines = vec![
                    Line::from(label).style(theme.header_style()),
                    Line::from(format!(
                        "› {}▏",
                        truncate_input_preview(&value, usize::from(width.saturating_sub(5)))
                    ))
                    .style(Style::default().add_modifier(Modifier::BOLD)),
                ];
                let has_error = error.is_some();
                if let Some(error) = error {
                    lines.push(Line::from(error).style(Style::default().fg(Color::Red)));
                }
                (title, lines, if has_error { 7 } else { 6 })
            }
            ViewDialog::Confirm { title, message } => (
                title,
                vec![Line::from(message).style(theme.panel_style(true))],
                5,
            ),
        };
        let hints = keybindings
            .iter()
            .map(|binding| format!("[{}] {}", binding.key(), binding.action()))
            .collect::<Vec<_>>()
            .join("   ");
        if !hints.is_empty() {
            lines.push(
                Line::from(truncate_to_width(
                    &hints,
                    usize::from(width.saturating_sub(4)),
                ))
                .style(theme.muted_style()),
            );
        }

        let height = height.min(area.height.saturating_sub(2));
        if height < 3 {
            return true;
        }
        let overlay = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, overlay);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: true })
                .block(Self::panel_block(title, true, theme)),
            overlay,
        );
        true
    }

    fn render_notification(&self, frame: &mut Frame<'_>, area: Rect) {
        let Some(notification) = self
            .notification
            .as_ref()
            .filter(|notification| notification.expires_at > Instant::now())
        else {
            return;
        };
        let width = area.width.saturating_sub(4).min(72);
        let height = 3;
        if width < 12 || area.height < height + 2 {
            return;
        }
        let overlay = Rect {
            x: area.x + area.width.saturating_sub(width) - 2,
            y: area.y + area.height.saturating_sub(height + 1),
            width,
            height,
        };
        let message =
            truncate_to_width(&notification.message, usize::from(width.saturating_sub(4)));
        frame.render_widget(Clear, overlay);
        frame.render_widget(
            Paragraph::new(Line::from(message).style(self.config.theme().header_style()))
                .block(Self::panel_block("Notice", true, self.config.theme())),
            overlay,
        );
    }

    fn capture_notification(&mut self) {
        let notification = self
            .selected
            .and_then(|index| self.views.get_mut(index))
            .and_then(|view| view.value_mut().take_notification());
        if let Some(message) = notification {
            self.notification = Some(WorkspaceNotification {
                message,
                expires_at: Instant::now() + NOTIFICATION_DURATION,
            });
        }
    }

    fn notification_timeout(&self) -> Option<Duration> {
        self.notification
            .as_ref()
            .map(|notification| {
                notification
                    .expires_at
                    .saturating_duration_since(Instant::now())
            })
            .filter(|remaining| !remaining.is_zero())
    }

    fn expire_notification(&mut self) -> bool {
        if self
            .notification
            .as_ref()
            .is_some_and(|notification| notification.expires_at <= Instant::now())
        {
            self.notification = None;
            true
        } else {
            false
        }
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
        if area.width < 12 || area.height < 5 {
            return;
        }
        let width = area.width.saturating_sub(2).min(88);
        let interior_width = usize::from(width.saturating_sub(4));
        let lines = self.help_lines(interior_width);
        let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
        let overlay = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, overlay);
        frame.render_widget(
            Paragraph::new(Text::from(lines)).block(Self::panel_block(
                "Shortcuts",
                true,
                self.config.theme(),
            )),
            overlay,
        );
    }

    fn footer_line(&self, width: usize) -> Line<'static> {
        let theme = self.config.theme();
        if width == 0 {
            return Line::default();
        }

        let mut bindings = self
            .selected
            .and_then(|index| self.views.get(index))
            .map(|view| view.value().keybindings(self.focus))
            .unwrap_or_default();
        let shell = shell_keybindings();
        let help = shell
            .iter()
            .find(|binding| binding.key() == "?")
            .cloned()
            .unwrap_or_else(|| crate::Keybinding::new("Help", "?"));
        bindings.extend(
            shell
                .into_iter()
                .filter(|binding| binding.key() == "Ctrl+P"),
        );
        bindings.sort_by_key(|binding| std::cmp::Reverse(binding.priority()));

        let mut visible = Vec::new();
        let mut help_label = "Help".to_owned();
        for _ in 0..4 {
            let help_text = footer_item_text(help.key(), &help_label);
            let candidate_space = width.saturating_sub(Line::from(help_text.as_str()).width());
            let mut next_visible = Vec::new();
            let mut used = 0usize;
            for binding in &bindings {
                let text = footer_item_text(binding.key(), binding.action());
                let item_width = Line::from(text.as_str()).width();
                let gap = usize::from(!next_visible.is_empty()) * 2;
                let trailing_gap = 2;
                if used
                    .saturating_add(gap)
                    .saturating_add(item_width)
                    .saturating_add(trailing_gap)
                    <= candidate_space
                {
                    used = used.saturating_add(gap).saturating_add(item_width);
                    next_visible.push(binding);
                }
            }
            let hidden = bindings.len().saturating_sub(next_visible.len());
            let next_label = if hidden == 0 {
                "Help".to_owned()
            } else {
                format!("Help +{hidden}")
            };
            if next_visible.len() == visible.len() && next_label == help_label {
                visible = next_visible;
                break;
            }
            visible = next_visible;
            help_label = next_label;
        }

        let mut spans = Vec::new();
        for binding in visible {
            if !spans.is_empty() {
                spans.push(Span::raw("  "));
            }
            spans.extend(footer_item_spans(binding.key(), binding.action(), theme));
        }
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.extend(footer_item_spans(help.key(), &help_label, theme));
        let line = Line::from(spans);
        if line.width() <= width {
            line
        } else {
            let short_help = footer_item_text(help.key(), "Help");
            Line::from(truncate_to_width(&short_help, width)).style(theme.header_style())
        }
    }

    fn help_lines(&self, width: usize) -> Vec<Line<'static>> {
        let theme = self.config.theme();
        let title = self
            .selected
            .and_then(|index| self.views.get(index))
            .map(|view| format!("{} actions", view.descriptor().title()))
            .unwrap_or_else(|| "View actions".to_owned());
        let view_bindings = self
            .selected
            .and_then(|index| self.views.get(index))
            .map(|view| view.value().keybindings(self.focus))
            .unwrap_or_default();
        let shell_bindings = shell_keybindings();

        if view_bindings.is_empty() {
            let key_width = widest_key_label(&shell_bindings).min(width.saturating_div(3).max(5));
            let mut lines = vec![Line::from(Span::styled("Workspace", theme.header_style()))];
            lines.extend(pack_keybindings(&shell_bindings, width, theme));
            lines.push(Line::from(Span::styled("Mouse", theme.header_style())));
            lines.push(help_binding_line(
                "Click",
                "Select / focus",
                key_width,
                width,
                theme,
            ));
            lines.push(help_binding_line(
                "Scroll",
                "Switch view",
                key_width,
                width,
                theme,
            ));
            return lines;
        }

        if width >= 64 {
            let column_width = width.saturating_sub(3) / 2;
            let key_width = widest_key_label(&view_bindings)
                .max(widest_key_label(&shell_bindings))
                .min(column_width.saturating_div(2).max(5));
            let title_width = Line::from(title.as_str()).width();
            let mut lines = vec![Line::from(vec![
                Span::styled(title, theme.header_style()),
                Span::raw(" ".repeat(column_width.saturating_sub(title_width).saturating_add(3))),
                Span::styled("Workspace", theme.header_style()),
            ])];
            for row in 0..view_bindings.len().max(shell_bindings.len()) {
                lines.push(help_grid_row(
                    view_bindings.get(row),
                    shell_bindings.get(row),
                    key_width,
                    column_width,
                    theme,
                ));
            }
            lines.push(Line::from(Span::styled("Mouse", theme.header_style())));
            lines.push(help_binding_line(
                "Click",
                "Select / focus",
                key_width,
                width,
                theme,
            ));
            lines.push(help_binding_line(
                "Scroll",
                "Switch view",
                key_width,
                width,
                theme,
            ));
            return lines;
        }

        let key_width = widest_key_label(&view_bindings)
            .max(widest_key_label(&shell_bindings))
            .min(width.saturating_div(3).max(5));
        let mut lines = vec![Line::from(Span::styled(title, theme.header_style()))];
        lines.extend(view_bindings.iter().map(|binding| {
            help_binding_line(binding.key(), binding.action(), key_width, width, theme)
        }));
        lines.push(Line::from(Span::styled("Workspace", theme.header_style())));
        lines.extend(shell_bindings.iter().map(|binding| {
            help_binding_line(binding.key(), binding.action(), key_width, width, theme)
        }));
        lines.push(Line::from(Span::styled("Mouse", theme.header_style())));
        lines.push(help_binding_line(
            "Click",
            "Select / focus",
            key_width,
            width,
            theme,
        ));
        lines.push(help_binding_line(
            "Scroll",
            "Switch view",
            key_width,
            width,
            theme,
        ));
        lines
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
                    label: title.to_owned(),
                    context: "View".to_owned(),
                    description: view.descriptor().description().to_owned(),
                    icon: view.value().icon(),
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
                    context: descriptor.title().to_owned(),
                    description: action.description().to_owned(),
                    icon: view.value().icon(),
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
                context: "Workspace".to_owned(),
                description: "Show details for the active view.".to_owned(),
                icon: icons::INSPECTOR,
                search_text: "toggle inspector details show active view".to_owned(),
                action: PaletteAction::ToggleInspector,
            },
            PaletteEntry {
                label: "Toggle drawer".to_owned(),
                context: "Workspace".to_owned(),
                description: "Open supplemental view content.".to_owned(),
                icon: icons::DRAWER,
                search_text: "toggle drawer supplemental open view content".to_owned(),
                action: PaletteAction::ToggleDrawer,
            },
            PaletteEntry {
                label: "Show help".to_owned(),
                context: "Workspace".to_owned(),
                description: "Show available keyboard and mouse shortcuts.".to_owned(),
                icon: icons::HELP,
                search_text: "show help keyboard shortcuts".to_owned(),
                action: PaletteAction::ShowHelp,
            },
            PaletteEntry {
                label: "Quit workspace".to_owned(),
                context: "Workspace".to_owned(),
                description: "Exit the workspace.".to_owned(),
                icon: icons::QUIT,
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
                self.tab_cursor = self.selected;
                self.focus = FocusArea::Content;
            }
            PaletteAction::ActivateViewAction { view_id, action_id } => {
                let index = self
                    .views
                    .iter()
                    .position(|view| view.descriptor().id() == &view_id);
                if let Some(index) = index {
                    self.selected = Some(index);
                    self.tab_cursor = Some(index);
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
        if area.width < 8 || area.height < 6 {
            return;
        }
        let entries = self.filtered_palette_entries();
        let width = area.width.saturating_sub(2).min(72);
        let inner_height = usize::from(area.height.saturating_sub(4));
        let show_detail = inner_height >= 3 && !entries.is_empty();
        let show_hints = inner_height >= 4;
        let visible_count = if entries.is_empty() {
            0
        } else {
            entries.len().min(7).min(
                inner_height
                    .saturating_sub(1 + usize::from(show_detail) + usize::from(show_hints))
                    .max(1),
            )
        };
        let line_count = 1
            + visible_count.max(usize::from(entries.is_empty()))
            + usize::from(show_detail)
            + usize::from(show_hints);
        let height = (line_count as u16 + 2).min(area.height.saturating_sub(2));
        let overlay = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };
        let theme = self.config.theme();
        let inner_width = usize::from(width.saturating_sub(2));
        let result_count = format!(
            "{} {}",
            entries.len(),
            if entries.len() == 1 {
                "action"
            } else {
                "actions"
            }
        );
        let mut lines = vec![search_prompt_line(
            " > ",
            &self.palette_query,
            &result_count,
            inner_width,
            theme,
        )];
        if entries.is_empty() {
            lines.push(Line::from("No matching actions").style(theme.muted_style()));
        } else {
            let start = self
                .palette_selected
                .saturating_sub(visible_count.saturating_sub(1));
            for (offset, entry) in entries.iter().skip(start).take(visible_count).enumerate() {
                let index = start + offset;
                lines.push(palette_entry_line(
                    entry,
                    index == self.palette_selected,
                    inner_width,
                    theme,
                ));
            }
            if show_detail && let Some(entry) = entries.get(self.palette_selected) {
                lines.push(
                    Line::from(truncate_to_width(&entry.description, inner_width))
                        .style(theme.muted_style()),
                );
            }
        }
        if show_hints {
            lines.push(popup_hints_line(
                theme,
                &[("↑/↓", "Move"), ("Enter", "Select"), ("Esc", "Close")],
            ));
        }
        frame.render_widget(Clear, overlay);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(Self::panel_block("Commands", true, theme))
                .wrap(ratatui::widgets::Wrap { trim: true }),
            overlay,
        );
    }

    fn panel_block<'a>(title: impl Into<Line<'a>>, focused: bool, theme: ThemePreset) -> Block<'a> {
        let mut title = title.into();
        if !title.spans.is_empty() {
            title.spans.insert(0, Span::raw(" "));
            title.spans.push(Span::raw(" "));
        }
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_style(focused))
            .title_style(theme.panel_style(focused))
            .title(title)
    }

    fn layout(
        area: Rect,
        inspector_open: bool,
        drawer_open: bool,
        preset: LayoutPreset,
        footer_height: u16,
    ) -> ShellRects {
        let [tabs, body, status] = Layout::vertical([
            Constraint::Length(TAB_ROW_HEIGHT),
            Constraint::Min(1),
            Constraint::Length(footer_height.max(1)),
        ])
        .areas(area);

        let can_show_panel = preset == LayoutPreset::Balanced
            && area.width > MIN_MAIN_WIDTH + SIDE_PANEL_WIDTH
            && area.height >= 8;
        let show_drawer = drawer_open && can_show_panel;
        let show_inspector = inspector_open && can_show_panel && !show_drawer;
        let state = ShellLayout {
            inspector: show_inspector,
            drawer: show_drawer,
        };
        let (main, inspector, drawer) = if can_show_panel && (show_inspector || show_drawer) {
            let [main, _gap, side_panel] = Layout::horizontal([
                Constraint::Min(MIN_MAIN_WIDTH),
                Constraint::Length(1),
                Constraint::Length(SIDE_PANEL_WIDTH),
            ])
            .areas(body);
            if show_drawer {
                (main, None, Some(side_panel))
            } else {
                (main, Some(side_panel), None)
            }
        } else {
            (body, None, None)
        };

        ShellRects {
            tabs,
            main,
            inspector,
            drawer,
            switcher: Rect::default(),
            status,
            state,
        }
    }

    fn normalize_focus(&mut self, layout: ShellLayout) {
        let visible = match self.focus {
            FocusArea::Navigation => true,
            FocusArea::Content => true,
            FocusArea::Inspector => layout.inspector,
            FocusArea::Drawer => layout.drawer,
        };
        if !visible {
            self.focus = FocusArea::Content;
        }
    }

    fn move_tab_cursor(&mut self, forward: bool) {
        let Some(current) = self.tab_cursor.or(self.selected) else {
            return;
        };
        let next = if forward {
            current
                .saturating_add(1)
                .min(self.views.len().saturating_sub(1))
        } else {
            current.saturating_sub(1)
        };
        self.tab_cursor = Some(next);
        self.selected = Some(next);
        self.focus = FocusArea::Navigation;
        self.ensure_tab_visible(self.last_regions.tabs.width);
    }

    fn activate_tab_cursor(&mut self) {
        if let Some(index) = self.tab_cursor.filter(|index| *index < self.views.len()) {
            self.activate_view(index);
        }
    }

    fn cancel_tab_navigation(&mut self) {
        self.tab_cursor = self.selected;
        self.focus = FocusArea::Content;
    }

    fn ensure_tab_visible(&mut self, width: u16) {
        let Some(target) = (if self.focus == FocusArea::Navigation {
            self.tab_cursor
        } else {
            self.selected
        }) else {
            return;
        };
        let viewport_width = usize::from(width);
        let tab_width = |index: usize| {
            self.views
                .get(index)
                .map(|view| {
                    Line::from(format!(
                        " {} {} ",
                        view.value().icon(),
                        view.descriptor().title()
                    ))
                    .width()
                })
                .unwrap_or(0)
        };
        let mut first = target;
        let mut used = tab_width(target);
        while first > 0 {
            let previous_width = tab_width(first - 1);
            if used.saturating_add(previous_width).saturating_add(1) > viewport_width {
                break;
            }
            first -= 1;
            used = used.saturating_add(previous_width).saturating_add(1);
        }
        self.tab_scroll = first;
    }
}

fn is_shift_view_switch_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::SHIFT)
        && matches!(key.code, KeyCode::Char('h' | 'l' | 'H' | 'L'))
}

fn is_alt_tab_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::ALT) && matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
}

fn shell_keybindings() -> Vec<crate::Keybinding> {
    vec![
        crate::Keybinding::new("Switch", "Shift+H/L").with_priority(70),
        crate::Keybinding::new("Picker", "Alt+Tab").with_priority(90),
        crate::Keybinding::new("Lists", "↑/↓, j/k").with_priority(35),
        crate::Keybinding::new("Details", "i").with_priority(20),
        crate::Keybinding::new("Side", "d").with_priority(15),
        crate::Keybinding::new("Commands", "Ctrl+P").with_priority(30),
        crate::Keybinding::new("Help", "?").with_priority(u8::MAX),
        crate::Keybinding::new("Quit", "q").with_priority(10),
    ]
}

fn widest_key_label(bindings: &[crate::Keybinding]) -> usize {
    bindings
        .iter()
        .map(|binding| Line::from(format!("[{}]", binding.key())).width())
        .max()
        .unwrap_or(5)
}

fn search_prompt_line(
    prefix: &str,
    query: &str,
    count: &str,
    width: usize,
    theme: ThemePreset,
) -> Line<'static> {
    let prefix_width = Line::from(prefix).width();
    let count_width = Line::from(count).width();
    let query_width = width.saturating_sub(prefix_width + count_width + 1);
    let query = truncate_to_width(query, query_width);
    let query_line_width = prefix_width + Line::from(query.as_str()).width();
    let padding = width.saturating_sub(query_line_width + count_width);
    Line::from(vec![
        Span::styled(prefix.to_owned(), theme.header_style()),
        Span::raw(query),
        Span::raw(" ".repeat(padding)),
        Span::styled(count.to_owned(), theme.muted_style()),
    ])
}

fn tab_label(
    icon: &str,
    title: &str,
    max_width: usize,
    label_style: ratatui::style::Style,
    icon_style: ratatui::style::Style,
) -> Option<Line<'static>> {
    const ICON_SLOT_WIDTH: usize = 3;
    const FIXED_WIDTH: usize = 6;
    if max_width < FIXED_WIDTH + 1 {
        return None;
    }

    let icon = truncate_to_width(icon, ICON_SLOT_WIDTH);
    let icon_width = Line::from(icon.as_str()).width().min(ICON_SLOT_WIDTH);
    let left_padding = (ICON_SLOT_WIDTH - icon_width) / 2;
    let right_padding = ICON_SLOT_WIDTH - icon_width - left_padding;
    let title = truncate_to_width(title, max_width - FIXED_WIDTH);

    Some(Line::from(vec![
        Span::styled(" ", label_style),
        Span::styled(" ".repeat(left_padding), label_style),
        Span::styled(icon, icon_style),
        Span::styled(" ".repeat(right_padding + 1), label_style),
        Span::styled(title, label_style),
        Span::styled(" ", label_style),
    ]))
}

fn popup_hints_line(theme: ThemePreset, hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::with_capacity(hints.len() * 3);
    for (index, (key, action)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled((*key).to_owned(), theme.header_style()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*action).to_owned(), theme.muted_style()));
    }
    Line::from(spans)
}

fn palette_entry_line(
    entry: &PaletteEntry,
    selected: bool,
    width: usize,
    theme: ThemePreset,
) -> Line<'static> {
    let selected_style = theme.navigation_item_style(true);
    let marker = if selected { "›" } else { " " };
    let prefix_width = Line::from(format!("{marker} {} ", entry.icon)).width();
    let context_width = Line::from(entry.context.as_str()).width();
    let label_width = width
        .saturating_sub(prefix_width + context_width + 2)
        .max(1);
    let label = truncate_to_width(&entry.label, label_width);
    let used_width = prefix_width + Line::from(label.as_str()).width();
    let context_fits = used_width + context_width + 2 <= width;
    let gap = if context_fits {
        width.saturating_sub(used_width + context_width)
    } else {
        0
    };
    let filler = if context_fits {
        0
    } else {
        width.saturating_sub(used_width)
    };

    let mut spans = vec![
        Span::styled(
            format!("{marker} "),
            if selected {
                selected_style
            } else {
                theme.accent_style()
            },
        ),
        Span::styled(
            format!("{} ", entry.icon),
            if selected {
                selected_style
            } else {
                theme.header_style()
            },
        ),
        Span::styled(
            label,
            if selected {
                selected_style
            } else {
                ratatui::style::Style::default()
            },
        ),
    ];
    if context_fits {
        spans.push(Span::styled(
            " ".repeat(gap),
            if selected {
                selected_style
            } else {
                ratatui::style::Style::default()
            },
        ));
        spans.push(Span::styled(
            entry.context.clone(),
            if selected {
                selected_style
            } else {
                theme.muted_style()
            },
        ));
    } else {
        spans.push(Span::styled(
            " ".repeat(filler),
            if selected {
                selected_style
            } else {
                ratatui::style::Style::default()
            },
        ));
    }
    Line::from(spans)
}

fn footer_item_text(key: &str, action: &str) -> String {
    format!("[{key}] {action}")
}

fn footer_item_spans(key: &str, action: &str, theme: ThemePreset) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("[{key}]"), theme.header_style()),
        Span::raw(" "),
        Span::styled(action.to_owned(), theme.muted_style()),
    ]
}

fn pack_keybindings(
    bindings: &[crate::Keybinding],
    width: usize,
    theme: ThemePreset,
) -> Vec<Line<'static>> {
    if bindings.is_empty() || width == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let mut line_width = 0usize;
    for binding in bindings {
        let key_text = format!("[{}]", binding.key());
        let key_width = Line::from(key_text.as_str()).width();
        let action_width = width.saturating_sub(key_width.saturating_add(1));
        let action_text = truncate_to_width(binding.action(), action_width);
        let item = vec![
            Span::styled(key_text, theme.header_style()),
            Span::raw(" "),
            Span::styled(action_text, theme.muted_style()),
        ];
        let item_width = Line::from(item.clone()).width().min(width);
        let gap = usize::from(!spans.is_empty()) * 2;
        if !spans.is_empty() && line_width.saturating_add(gap).saturating_add(item_width) > width {
            lines.push(Line::from(std::mem::take(&mut spans)));
            line_width = 0;
        }
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
            line_width = line_width.saturating_add(2);
        }
        spans.extend(item);
        line_width = line_width.saturating_add(item_width);
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

fn help_binding_line(
    key: &str,
    action: &str,
    key_width: usize,
    width: usize,
    theme: ThemePreset,
) -> Line<'static> {
    let key_text = if key.is_empty() {
        String::new()
    } else {
        format!("[{key}]")
    };
    let key_text = truncate_to_width(&key_text, key_width);
    let key_display_width = Line::from(key_text.as_str()).width();
    let padding = key_width.saturating_sub(key_display_width);
    let action_width = width.saturating_sub(key_width.saturating_add(1));
    let action = truncate_to_width(action, action_width);
    Line::from(vec![
        Span::styled(key_text, theme.header_style()),
        Span::raw(format!("{} ", " ".repeat(padding))),
        Span::styled(action, theme.muted_style()),
    ])
}

fn help_grid_row(
    left: Option<&crate::Keybinding>,
    right: Option<&crate::Keybinding>,
    key_width: usize,
    column_width: usize,
    theme: ThemePreset,
) -> Line<'static> {
    let left = left.map_or_else(
        || Line::from(""),
        |binding| {
            help_binding_line(
                binding.key(),
                binding.action(),
                key_width,
                column_width,
                theme,
            )
        },
    );
    let left_width = left.width();
    let mut spans = left.spans;
    spans.push(Span::raw(
        " ".repeat(column_width.saturating_sub(left_width).saturating_add(3)),
    ));
    if let Some(binding) = right {
        spans.extend(
            help_binding_line(
                binding.key(),
                binding.action(),
                key_width,
                column_width,
                theme,
            )
            .spans,
        );
    }
    Line::from(spans)
}

fn truncate_to_width(value: &str, max_width: usize) -> String {
    if Line::from(value).width() <= max_width {
        return value.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }

    let mut result = String::new();
    for character in value.chars() {
        let mut candidate = result.clone();
        candidate.push(character);
        if Line::from(format!("{candidate}…")).width() > max_width {
            break;
        }
        result.push(character);
    }
    result.push('…');
    result
}

fn truncate_input_preview(value: &str, max_width: usize) -> String {
    if Line::from(value).width() <= max_width {
        return value.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }

    let content_width = max_width.saturating_sub(1);
    let mut visible_reversed = Vec::new();
    let mut visible_width = 0usize;
    for character in value.chars().rev() {
        let character_width = Line::from(character.to_string()).width();
        if visible_width.saturating_add(character_width) > content_width {
            break;
        }
        visible_reversed.push(character);
        visible_width += character_width;
    }

    let mut preview = String::from("…");
    preview.extend(visible_reversed.into_iter().rev());
    preview
}

#[derive(Clone, Copy, Debug, Default)]
struct ShellRects {
    tabs: Rect,
    main: Rect,
    inspector: Option<Rect>,
    drawer: Option<Rect>,
    switcher: Rect,
    status: Rect,
    state: ShellLayout,
}

/// Starts the workspace event loop, managing terminal setup and restoration.
pub fn run(mut workspace: Workspace) -> io::Result<Workspace> {
    require_interactive_terminal()?;

    let _mouse_capture = MouseCaptureGuard;
    crossterm::execute!(io::stdout(), EnableMouseCapture)?;

    ratatui::run(|terminal| {
        'redraw: loop {
            terminal.draw(|frame| workspace.render(frame))?;
            loop {
                let timeout = if workspace.selected_view_has_background_work() {
                    Duration::from_millis(100)
                } else {
                    workspace
                        .notification_timeout()
                        .unwrap_or(Duration::from_secs(30))
                };
                if !event::poll(timeout)? {
                    if workspace.selected_view_has_background_work()
                        || workspace.expire_notification()
                    {
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
                    Event::Mouse(mouse) => {
                        if workspace.handle_mouse(mouse) {
                            break;
                        }
                    }
                    Event::Resize(_, _) => break,
                    _ => {}
                }
            }
        }
        Ok::<(), io::Error>(())
    })?;
    Ok(workspace)
}

struct MouseCaptureGuard;

impl Drop for MouseCaptureGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(io::stdout(), DisableMouseCapture);
    }
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
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
    fn input_preview_keeps_the_cursor_visible_for_long_values() {
        assert_eq!(truncate_input_preview("abcdef", 4), "…def");
        assert_eq!(truncate_input_preview("short", 8), "short");
    }

    #[test]
    fn tabs_remain_visible_and_context_panel_uses_the_right_side() {
        let layout = Workspace::layout(
            Rect::new(0, 0, 120, 36),
            true,
            false,
            LayoutPreset::Balanced,
            1,
        );
        assert_eq!(layout.tabs.y, 0);
        assert_eq!(layout.tabs.height, TAB_ROW_HEIGHT);
        assert!(layout.state.inspector);
        assert_eq!(layout.main.x, layout.tabs.x);
        assert!(layout.inspector.unwrap().x > layout.main.x);
        assert_eq!(layout.status.height, 1);
    }

    #[test]
    fn context_panel_is_hidden_when_space_or_layout_does_not_allow_it() {
        let narrow = Workspace::layout(
            Rect::new(0, 0, 60, 20),
            true,
            false,
            LayoutPreset::Balanced,
            2,
        );
        let focus = Workspace::layout(
            Rect::new(0, 0, 120, 36),
            true,
            false,
            LayoutPreset::Focus,
            2,
        );
        assert!(!narrow.state.inspector);
        assert!(narrow.inspector.is_none());
        assert!(!focus.state.inspector);
        assert!(focus.inspector.is_none());
    }

    #[test]
    fn inspector_and_drawer_share_one_right_side_panel() {
        let layout = Workspace::layout(
            Rect::new(0, 0, 120, 36),
            true,
            true,
            LayoutPreset::Balanced,
            2,
        );
        assert!(!layout.state.inspector);
        assert!(layout.state.drawer);
        assert!(layout.inspector.is_none());
        assert!(layout.drawer.is_some());
    }
}
