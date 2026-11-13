use chrono::{Datelike, Days, Months, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Paragraph, Wrap},
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, View};

/// Calendar month view with keyboard date selection.
pub struct CalendarView {
    descriptor: ViewDescriptor,
    today: NaiveDate,
    selected_date: NaiveDate,
}

impl CalendarView {
    /// Creates a calendar positioned on the supplied local date.
    pub fn new(today: NaiveDate) -> Self {
        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("calendar").expect("static view identifier is valid"),
                "Calendar",
                "Browse dates and review due tasks",
            ),
            today,
            selected_date: today,
        }
    }

    /// Returns the currently selected date.
    pub fn selected_date(&self) -> NaiveDate {
        self.selected_date
    }

    fn month_lines(&self) -> Vec<Line<'static>> {
        let first_day = self.selected_date.with_day(1).unwrap_or(self.selected_date);
        let weekday_offset = first_day.weekday().num_days_from_monday();
        let days_in_month = days_in_month(self.selected_date);
        let weeks = (weekday_offset + days_in_month).div_ceil(7);
        let mut lines = vec![
            Line::from(self.selected_date.format("%B %Y").to_string())
                .style(Style::default().add_modifier(Modifier::BOLD)),
            Line::from("←/→ day  ↑/↓ week  PgUp/PgDn month  t today"),
            Line::from("Mo   Tu   We   Th   Fr   Sa   Su"),
        ];

        for week in 0..weeks {
            let cells = (0..7)
                .map(|weekday| {
                    let day = week * 7 + weekday + 1;
                    if day <= weekday_offset || day > weekday_offset + days_in_month {
                        "    ".to_owned()
                    } else {
                        let day = day - weekday_offset;
                        if day == self.selected_date.day() {
                            format!("[{day:>2}]")
                        } else {
                            format!(" {day:>2} ")
                        }
                    }
                })
                .collect::<Vec<_>>();
            lines.push(Line::from(cells.join(" ")));
        }

        lines.push(Line::from(format!(
            "Selected date: {}",
            self.selected_date.format("%Y-%m-%d")
        )));
        lines
    }
}

fn days_in_month(date: NaiveDate) -> u32 {
    match date.month() {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if date.year().rem_euclid(400) == 0
            || (date.year().rem_euclid(4) == 0 && date.year().rem_euclid(100) != 0) =>
        {
            29
        }
        2 => 28,
        _ => 0,
    }
}

impl View for CalendarView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(
            Paragraph::new(self.month_lines()).wrap(Wrap { trim: true }),
            area,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, focus: FocusArea) -> bool {
        if focus != FocusArea::Content {
            return false;
        }

        let selected = match key.code {
            KeyCode::Left | KeyCode::Char('h') if key.modifiers.is_empty() => {
                self.selected_date.checked_sub_days(Days::new(1))
            }
            KeyCode::Right | KeyCode::Char('l') if key.modifiers.is_empty() => {
                self.selected_date.checked_add_days(Days::new(1))
            }
            KeyCode::Up | KeyCode::Char('k') if key.modifiers.is_empty() => {
                self.selected_date.checked_sub_days(Days::new(7))
            }
            KeyCode::Down | KeyCode::Char('j') if key.modifiers.is_empty() => {
                self.selected_date.checked_add_days(Days::new(7))
            }
            KeyCode::PageUp => self.selected_date.checked_sub_months(Months::new(1)),
            KeyCode::PageDown => self.selected_date.checked_add_months(Months::new(1)),
            KeyCode::Char('t') if key.modifiers.is_empty() => Some(self.today),
            _ => return false,
        };

        if let Some(selected) = selected {
            self.selected_date = selected;
        }
        true
    }
}
