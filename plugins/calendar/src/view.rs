use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use chrono::{Datelike, Days, Local, Months, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use tuilane_core::{EventBus, EventSubscription, ServiceBroker, ServiceBrokerError};
use tuilane_plugin_api::tasks::{TaskDate, TaskSummary, TasksByDateV1, TasksChangedV1};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{
    DashboardItem, DashboardLayout, DashboardOption, DashboardOptionKind, DashboardOptionValue,
    DashboardSize, DashboardTone, DashboardWidget, DashboardWidgetContent,
    DashboardWidgetDescriptor, DashboardWidgetId, FocusArea, Keybinding, ThemePreset, View,
    render_empty_state,
};

/// Calendar month view with keyboard date selection and an optional Tasks inspector.
pub struct CalendarView {
    descriptor: ViewDescriptor,
    selected_date: NaiveDate,
    services: Rc<RefCell<ServiceBroker>>,
    tasks: Option<Result<Vec<TaskSummary>, String>>,
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
    day_hits: Vec<(NaiveDate, Rect)>,
}

impl CalendarView {
    /// Creates a calendar positioned on the supplied local date.
    pub fn new(
        initial_date: NaiveDate,
        services: Rc<RefCell<ServiceBroker>>,
        events: &EventBus,
    ) -> Self {
        let dirty = Rc::new(Cell::new(true));
        let event_dirty = Rc::clone(&dirty);
        let event_subscription = events.subscribe(move |_: &TasksChangedV1| {
            event_dirty.set(true);
        });

        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("calendar").expect("static view identifier is valid"),
                "Calendar",
                "Browse dates and review due tasks",
            ),
            selected_date: initial_date,
            services,
            tasks: None,
            dirty,
            _event_subscription: event_subscription,
            day_hits: Vec::new(),
        }
    }

    /// Returns the currently selected date.
    pub fn selected_date(&self) -> NaiveDate {
        self.selected_date
    }

    fn render_month(&mut self, frame: &mut Frame<'_>, area: Rect, theme: ThemePreset) {
        self.day_hits.clear();
        let title = self.selected_date.format("%B %Y").to_string();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_style(false))
            .title(Line::from(format!(" {} ", title)).style(theme.header_style()));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let first_day = self.selected_date.with_day(1).unwrap_or(self.selected_date);
        let weekday_offset = first_day.weekday().num_days_from_monday();
        let days_in_month = days_in_month(self.selected_date);
        let weeks = (weekday_offset + days_in_month).div_ceil(7);
        if inner.width < 21 || u32::from(inner.height) < weeks + 1 {
            let (title, description) = match inner.width {
                0..=5 => ("!", ""),
                6..=14 => ("Resize", ""),
                15..=17 => ("Expand", "Resize terminal"),
                18..=20 => ("Expand calendar", "See full month"),
                _ => ("Calendar needs more space", "Resize to see the full month."),
            };
            render_empty_state(frame, inner, title, description);
            return;
        }

        let columns = Layout::horizontal([Constraint::Ratio(1, 7); 7]).split(inner);
        let weekdays = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
        for (column, weekday) in columns.iter().zip(weekdays) {
            frame.render_widget(
                Paragraph::new(weekday)
                    .alignment(Alignment::Center)
                    .style(theme.muted_style()),
                Rect::new(column.x, inner.y, column.width, 1),
            );
        }

        let mut y = inner.y.saturating_add(1);
        for week in 0..weeks {
            if y >= inner.y.saturating_add(inner.height) {
                break;
            }
            for (weekday, column) in columns.iter().enumerate() {
                let day_index = week * 7 + weekday as u32;
                if day_index < weekday_offset {
                    continue;
                }
                let day = day_index - weekday_offset + 1;
                if day > days_in_month {
                    continue;
                }
                let date = first_day
                    .checked_add_days(Days::new(u64::from(day - 1)))
                    .unwrap_or(first_day);
                let cell = Rect::new(column.x, y, column.width, 1);
                let style = if date == self.selected_date {
                    theme.navigation_item_style(true)
                } else if date == Local::now().date_naive() {
                    theme.accent_style().add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default()
                };
                frame.render_widget(
                    Paragraph::new(format!("{day:>2}"))
                        .alignment(Alignment::Center)
                        .style(style),
                    cell,
                );
                self.day_hits.push((date, cell));
            }
            y = y.saturating_add(1);
        }

        if y < inner.y.saturating_add(inner.height) {
            frame.render_widget(
                Paragraph::new(format!(
                    "Selected · {}",
                    self.selected_date.format("%a, %b %-d")
                ))
                .style(theme.accent_style()),
                Rect::new(
                    inner.x,
                    y.saturating_add(1)
                        .min(inner.y.saturating_add(inner.height).saturating_sub(1)),
                    inner.width,
                    1,
                ),
            );
        }
    }

    fn render_agenda(&mut self, frame: &mut Frame<'_>, area: Rect, theme: ThemePreset) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_style(false))
            .title(Line::from(" Day agenda ").style(theme.header_style()));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let mut lines = vec![
            Line::from(self.selected_date.format("%A, %B %-d").to_string())
                .style(theme.header_style()),
        ];
        match self.tasks.as_ref() {
            Some(Ok(tasks)) if tasks.is_empty() => {
                lines.push(Line::default());
                lines.push(Line::from("No tasks due on this date.").style(theme.muted_style()));
            }
            Some(Ok(tasks)) => {
                lines.push(
                    Line::from(format!("{} tasks due", tasks.len())).style(theme.muted_style()),
                );
                lines.push(Line::default());
                lines.extend(tasks.iter().map(|task| {
                    let (icon, style) =
                        if task.status() == tuilane_plugin_api::tasks::TaskStatus::Completed {
                            (tuilane_ui::icons::TASK_DONE, theme.positive_style())
                        } else {
                            (tuilane_ui::icons::TASK_OPEN, theme.accent_style())
                        };
                    Line::from(vec![
                        ratatui::text::Span::styled(icon, style),
                        ratatui::text::Span::raw("  "),
                        ratatui::text::Span::raw(task.title().as_str()),
                    ])
                }));
            }
            Some(Err(error)) => {
                lines.push(Line::default());
                lines.push(Line::from(error.as_str()).style(theme.muted_style()));
            }
            None => lines.push(Line::from("No task data yet.").style(theme.muted_style())),
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
    }

    fn refresh_tasks(&mut self) {
        if !self.dirty.replace(false) {
            return;
        }

        let date = self.selected_date.format("%Y-%m-%d").to_string();
        let request_date = match TaskDate::parse(date) {
            Ok(date) => date,
            Err(error) => {
                self.tasks = Some(Err(format!("Selected date cannot be queried: {error}")));
                return;
            }
        };

        let Ok(mut services) = self.services.try_borrow_mut() else {
            self.dirty.set(true);
            self.tasks = Some(Err("Tasks query is busy. Try again shortly.".to_owned()));
            return;
        };

        let result = match services.call(TasksByDateV1::new(request_date)) {
            Ok(tasks) => Ok(tasks),
            Err(ServiceBrokerError::MissingProvider(capability)) => {
                let incompatible = services
                    .discover()
                    .iter()
                    .any(|available| available.name() == capability.name());
                if incompatible {
                    Err("Tasks service version is incompatible.".to_owned())
                } else {
                    Err("Tasks service is unavailable.".to_owned())
                }
            }
            Err(error) => Err(format!("Tasks query failed: {error}")),
        };
        self.tasks = Some(result);
    }

    fn inspector_lines(&mut self) -> Vec<Line<'static>> {
        self.refresh_tasks();
        let mut lines = vec![Line::from(format!(
            "Tasks due {}:",
            self.selected_date.format("%Y-%m-%d")
        ))];

        match self.tasks.as_ref() {
            Some(Ok(tasks)) if tasks.is_empty() => {
                lines.push(Line::from("No tasks due on this date."));
            }
            Some(Ok(tasks)) => {
                lines.extend(tasks.iter().map(|task| {
                    Line::from(format!("{} ({})", task.title(), task.status().as_str()))
                }));
            }
            Some(Err(error)) => lines.push(Line::from(error.clone())),
            None => lines.push(Line::from("Tasks have not been queried.")),
        }

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

    fn icon(&self) -> &'static str {
        tuilane_ui::icons::CALENDAR
    }

    fn keybindings(&self, focus: FocusArea) -> Vec<Keybinding> {
        if focus != FocusArea::Content {
            return Vec::new();
        }

        vec![
            Keybinding::new("Day", "←/→, h/l").with_priority(10),
            Keybinding::new("Week", "↑/↓, j/k").with_priority(10),
            Keybinding::new("Month", "PgUp/PgDn").with_priority(10),
            Keybinding::new("Today", "t").with_priority(90),
        ]
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.render_with_theme(frame, area, ThemePreset::default());
    }

    fn render_with_theme(&mut self, frame: &mut Frame<'_>, area: Rect, theme: ThemePreset) {
        self.refresh_tasks();
        if area.width >= 84 {
            let regions = Layout::horizontal([
                Constraint::Percentage(68),
                Constraint::Length(1),
                Constraint::Percentage(32),
            ])
            .split(area);
            self.render_month(frame, regions[0], theme);
            self.render_agenda(frame, regions[2], theme);
        } else {
            self.render_month(frame, area, theme);
        }
    }

    fn handle_mouse(&mut self, event: MouseEvent, focus: FocusArea) -> bool {
        if focus != FocusArea::Content || event.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        let Some((date, _)) = self.day_hits.iter().find(|(_, area)| {
            event.column >= area.x
                && event.column < area.x.saturating_add(area.width)
                && event.row >= area.y
                && event.row < area.y.saturating_add(area.height)
        }) else {
            return false;
        };
        self.selected_date = *date;
        self.dirty.set(true);
        true
    }

    fn dashboard_widgets(&mut self, layout: &DashboardLayout) -> Vec<DashboardWidget> {
        self.refresh_tasks();
        let id = DashboardWidgetId::new("tuilane.calendar.selected-day")
            .expect("static dashboard widget identifier is valid");
        let descriptor = DashboardWidgetDescriptor::new(
            id.clone(),
            "On this day",
            "Tasks due on the selected calendar date",
            DashboardSize::Wide,
            vec![DashboardOption::new(
                "limit",
                "Visible tasks",
                DashboardOptionKind::Number {
                    default: 5,
                    min: 1,
                    max: 10,
                },
            )],
        );
        let limit = layout
            .preference(&id)
            .and_then(|preference| preference.option("limit"))
            .and_then(|value| match value {
                DashboardOptionValue::Number(value) => Some(usize::from((*value).clamp(1, 10))),
                _ => None,
            })
            .unwrap_or(5);
        let content = match self.tasks.as_ref() {
            Some(Ok(tasks)) => DashboardWidgetContent::List {
                items: tasks
                    .iter()
                    .take(limit)
                    .map(|task| {
                        let tone = if task.status().as_str() == "completed" {
                            DashboardTone::Positive
                        } else {
                            DashboardTone::Default
                        };
                        DashboardItem::new(task.title().as_str(), None).with_tone(tone)
                    })
                    .collect(),
                empty_message: format!("No tasks due {}", self.selected_date.format("%b %-d"))
                    .into_boxed_str(),
            },
            Some(Err(error)) => DashboardWidgetContent::Message {
                title: "Calendar data unavailable".into(),
                detail: error.as_str().into(),
                tone: DashboardTone::Warning,
            },
            None => DashboardWidgetContent::Message {
                title: "Calendar is loading".into(),
                detail: "Task data will appear after refresh.".into(),
                tone: DashboardTone::Muted,
            },
        };
        vec![DashboardWidget::new(descriptor, content)]
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
            KeyCode::Up => self.selected_date.checked_sub_days(Days::new(7)),
            KeyCode::Down => self.selected_date.checked_add_days(Days::new(7)),
            KeyCode::PageUp => self.selected_date.checked_sub_months(Months::new(1)),
            KeyCode::PageDown => self.selected_date.checked_add_months(Months::new(1)),
            KeyCode::Char('t') if key.modifiers.is_empty() => Some(Local::now().date_naive()),
            _ => return false,
        };

        if let Some(selected) = selected {
            self.selected_date = selected;
            self.dirty.set(true);
        }
        true
    }

    fn render_inspector(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
        frame.render_widget(
            Paragraph::new(self.inspector_lines()).wrap(Wrap { trim: true }),
            area,
        );
        true
    }
}
