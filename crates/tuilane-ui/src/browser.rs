//! Shared list and detail presentation for data-oriented workspace views.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{ThemePreset, render_empty_state};

/// Semantic emphasis for a status or category label shown by a browser row.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BrowserTone {
    /// Use the regular text style.
    #[default]
    Default,
    /// Emphasize content with the active theme's accent.
    Accent,
    /// Mark a successful or completed state.
    Positive,
    /// Mark a state that needs attention.
    Warning,
    /// Mark a failed or critical state.
    Critical,
}

/// One selectable row in a shared workspace browser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserItem {
    icon: &'static str,
    title: String,
    summary: Option<String>,
    badge: Option<(String, BrowserTone)>,
}

impl BrowserItem {
    /// Creates a row with the shared fallback icon.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            icon: crate::icons::DEFAULT,
            title: title.into(),
            summary: None,
            badge: None,
        }
    }

    /// Sets a one-cell Nerd Font icon for the row.
    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }

    /// Adds a muted second line beneath the row title.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Adds a compact status such as `Open` or `Done` beside the title.
    pub fn with_status(mut self, label: impl Into<String>, tone: BrowserTone) -> Self {
        self.badge = Some((label.into(), tone));
        self
    }

    /// Adds a compact category or context label beside the title.
    pub fn with_badge(mut self, label: impl Into<String>, tone: BrowserTone) -> Self {
        self.badge = Some((label.into(), tone));
        self
    }
}

/// Selected-item content displayed in the browser's detail panel.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BrowserDetails {
    eyebrow: Option<String>,
    title: String,
    summary: Option<String>,
    fields: Vec<(String, String)>,
    body: Vec<String>,
}

impl BrowserDetails {
    /// Creates the detail content for one selected item.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    /// Adds a small context label above the selected item's title.
    pub fn with_eyebrow(mut self, eyebrow: impl Into<String>) -> Self {
        self.eyebrow = Some(eyebrow.into());
        self
    }

    /// Adds a short description under the title.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Adds a labeled metadata row.
    pub fn with_field(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push((label.into(), value.into()));
        self
    }

    /// Adds multiline content below the metadata.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body.push(body.into());
        self
    }
}

/// Tracks the visible browser rows so a containing view can route mouse clicks.
#[derive(Clone, Debug, Default)]
pub struct BrowserHitMap {
    rows: Vec<(usize, Rect)>,
}

impl BrowserHitMap {
    /// Returns the row index under a terminal position, if one is visible.
    pub fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        self.rows.iter().find_map(|(index, area)| {
            (column >= area.x
                && column < area.x.saturating_add(area.width)
                && row >= area.y
                && row < area.y.saturating_add(area.height))
            .then_some(*index)
        })
    }

    /// Clears stale mouse targets when the view changes modes or has no items.
    pub fn clear(&mut self) {
        self.rows.clear();
    }
}

/// Renders a responsive list and selected-item detail view.
#[allow(clippy::too_many_arguments)]
pub fn render_browser(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: ThemePreset,
    items: &[BrowserItem],
    selected: Option<usize>,
    details: Option<&BrowserDetails>,
    empty_title: &str,
    empty_description: &str,
    hit_map: &mut BrowserHitMap,
) {
    hit_map.clear();
    if area.is_empty() {
        return;
    }
    if area.width < 8 || area.height < 4 {
        let message = if area.width >= 6 { "Resize" } else { "!" };
        let message_area = Rect::new(area.x, area.y + area.height / 2, area.width, 1);
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(theme.muted_style()),
            message_area,
        );
        return;
    }
    if items.is_empty() {
        render_empty_state(frame, area, empty_title, empty_description);
        return;
    }

    let (list_area, detail_area) = if area.width >= 88 {
        let regions = Layout::horizontal([
            Constraint::Percentage(38),
            Constraint::Length(1),
            Constraint::Percentage(62),
        ])
        .split(area);
        (regions[0], Some(regions[2]))
    } else if area.height >= 16 {
        let regions = Layout::vertical([
            Constraint::Percentage(46),
            Constraint::Length(1),
            Constraint::Percentage(54),
        ])
        .split(area);
        (regions[0], Some(regions[2]))
    } else {
        (area, None)
    };

    render_item_list(frame, list_area, theme, items, selected, hit_map);
    if let Some(detail_area) = detail_area {
        render_detail(frame, detail_area, theme, details);
    }
}

/// Applies row selection from a left click to a browser's current focus.
pub fn select_browser_row(
    hit_map: &BrowserHitMap,
    selected: &mut Option<usize>,
    event: MouseEvent,
) -> bool {
    if event.kind != MouseEventKind::Down(MouseButton::Left) {
        return false;
    }
    let Some(index) = hit_map.row_at(event.column, event.row) else {
        return false;
    };
    *selected = Some(index);
    true
}

fn render_item_list(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: ThemePreset,
    items: &[BrowserItem],
    selected: Option<usize>,
    hit_map: &mut BrowserHitMap,
) {
    let block = browser_block(theme, format!(" Items · {} ", items.len()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = items
        .iter()
        .map(|item| {
            let mut title = vec![
                Span::styled(item.icon, theme.accent_style()),
                Span::raw("  "),
                Span::raw(safe_text(&item.title)),
            ];
            if let Some((status, tone)) = &item.badge {
                title.push(Span::raw("  "));
                title.push(Span::styled(
                    safe_text(status),
                    browser_tone_style(theme, *tone).add_modifier(Modifier::BOLD),
                ));
            }
            let summary = item.summary.as_deref().map_or_else(Line::default, |value| {
                Line::from(format!("   {}", safe_text(value))).style(theme.muted_style())
            });
            ListItem::new(vec![Line::from(title), summary])
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(selected.filter(|index| *index < items.len()));
    frame.render_stateful_widget(
        List::new(rows)
            .highlight_symbol("› ")
            .highlight_style(theme.navigation_item_style(true)),
        inner,
        &mut state,
    );

    let mut y = inner.y;
    let bottom = inner.y.saturating_add(inner.height);
    for index in state.offset()..items.len() {
        if y >= bottom {
            break;
        }
        let height = 2.min(bottom.saturating_sub(y));
        hit_map
            .rows
            .push((index, Rect::new(inner.x, y, inner.width, height)));
        y = y.saturating_add(2);
    }
}

fn render_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: ThemePreset,
    details: Option<&BrowserDetails>,
) {
    let block = browser_block(theme, " Details ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(details) = details else {
        render_empty_state(
            frame,
            inner,
            "Select an item",
            "Its details will appear here.",
        );
        return;
    };

    let mut lines = Vec::new();
    if let Some(eyebrow) = &details.eyebrow {
        lines.push(Line::from(safe_text(eyebrow)).style(theme.muted_style()));
    }
    lines.push(
        Line::from(safe_text(&details.title))
            .style(theme.header_style().add_modifier(Modifier::BOLD)),
    );
    if let Some(summary) = &details.summary {
        lines.push(Line::from(safe_text(summary)).style(theme.muted_style()));
    }
    if !details.fields.is_empty() || !details.body.is_empty() {
        lines.push(Line::default());
    }
    for (label, value) in &details.fields {
        lines.push(Line::from(vec![
            Span::styled(format!("{}  ", safe_text(label)), theme.accent_style()),
            Span::raw(safe_text(value)),
        ]));
    }
    for body in &details.body {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.extend(body.lines().map(|line| Line::from(safe_text(line))));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn browser_block(theme: ThemePreset, title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_style(false))
        .title(title.into().style(theme.header_style()))
}

fn browser_tone_style(theme: ThemePreset, tone: BrowserTone) -> Style {
    match tone {
        BrowserTone::Default => Style::default(),
        BrowserTone::Accent => theme.accent_style(),
        BrowserTone::Positive => theme.positive_style(),
        BrowserTone::Warning => theme.warning_style(),
        BrowserTone::Critical => theme.critical_style(),
    }
}

fn safe_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseEvent};
    use ratatui::style::Color;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    fn mouse(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn screen(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn renders_an_index_and_detail_pane_on_wide_terminals() {
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        let items = vec![BrowserItem::new("Active task").with_summary("Due tomorrow")];
        let details = BrowserDetails::new("Active task")
            .with_eyebrow("TASK")
            .with_field("Status", "Open")
            .with_body("A longer note about this task.");
        terminal
            .draw(|frame| {
                render_browser(
                    frame,
                    frame.area(),
                    ThemePreset::Default,
                    &items,
                    Some(0),
                    Some(&details),
                    "No tasks",
                    "Create one to begin.",
                    &mut BrowserHitMap::default(),
                )
            })
            .unwrap();

        let display = screen(&terminal);
        assert!(display.contains("Items · 1"));
        assert!(display.contains("Details"));
        assert!(display.contains("Due tomorrow"));
        assert!(display.contains("A longer note about this task."));
    }

    #[test]
    fn stacks_panels_on_narrow_terminals_and_maps_visible_rows() {
        let mut terminal = Terminal::new(TestBackend::new(70, 24)).unwrap();
        let items = vec![
            BrowserItem::new("First item"),
            BrowserItem::new("Second item"),
            BrowserItem::new("Third item"),
        ];
        let mut hit_map = BrowserHitMap::default();
        terminal
            .draw(|frame| {
                render_browser(
                    frame,
                    frame.area(),
                    ThemePreset::HighContrast,
                    &items,
                    Some(1),
                    Some(&BrowserDetails::new("Second item")),
                    "No items",
                    "Create one to begin.",
                    &mut hit_map,
                )
            })
            .unwrap();

        let display = screen(&terminal);
        assert!(display.contains("Second item"));
        assert!(display.contains("Details"));
        let row = hit_map.rows[1].1.y;
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|cell| { cell.bg == Color::Yellow && cell.symbol() == "S" })
        );
        let mut selected = Some(0);
        assert!(select_browser_row(&hit_map, &mut selected, mouse(5, row)));
        assert_eq!(selected, Some(1));
        assert!(!select_browser_row(
            &hit_map,
            &mut selected,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 5,
                row,
                modifiers: KeyModifiers::NONE,
            }
        ));
    }

    #[test]
    fn removes_terminal_control_characters_from_user_content() {
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        let items = vec![BrowserItem::new("Task\u{1b}[31m")];
        terminal
            .draw(|frame| {
                render_browser(
                    frame,
                    frame.area(),
                    ThemePreset::Default,
                    &items,
                    Some(0),
                    Some(&BrowserDetails::new("Task\u{7}")),
                    "No tasks",
                    "Create one to begin.",
                    &mut BrowserHitMap::default(),
                )
            })
            .unwrap();

        let display = screen(&terminal);
        assert!(display.contains("Task[31m"));
        assert!(!display.contains('\u{1b}'));
        assert!(!display.contains('\u{7}'));
    }

    #[test]
    fn shows_a_compact_resize_hint_when_the_browser_is_too_small() {
        let mut terminal = Terminal::new(TestBackend::new(12, 3)).unwrap();
        terminal
            .draw(|frame| {
                render_browser(
                    frame,
                    frame.area(),
                    ThemePreset::Default,
                    &[],
                    None,
                    None,
                    "No items",
                    "Create one to begin.",
                    &mut BrowserHitMap::default(),
                )
            })
            .unwrap();

        assert!(screen(&terminal).contains("Resize"));
    }

    #[test]
    fn high_contrast_theme_uses_brighter_semantic_status_colors() {
        assert_eq!(
            browser_tone_style(ThemePreset::HighContrast, BrowserTone::Positive).fg,
            Some(Color::LightGreen)
        );
        assert_eq!(
            browser_tone_style(ThemePreset::HighContrast, BrowserTone::Warning).fg,
            Some(Color::LightYellow)
        );
        assert_eq!(
            browser_tone_style(ThemePreset::HighContrast, BrowserTone::Critical).fg,
            Some(Color::LightRed)
        );
    }
}
