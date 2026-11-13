use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use chrono::{Datelike, Days, Local, Months, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Paragraph, Wrap},
};
use tuilane_core::{EventBus, EventSubscription, ServiceBroker, ServiceBrokerError};
use tuilane_plugin_api::tasks::{TaskDate, TaskSummary, TasksByDateV1, TasksChangedV1};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, View};

/// Calendar month view with keyboard date selection and an optional Tasks inspector.
pub struct CalendarView {
    descriptor: ViewDescriptor,
    selected_date: NaiveDate,
    services: Rc<RefCell<ServiceBroker>>,
    tasks: Option<Result<Vec<TaskSummary>, String>>,
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
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
            KeyCode::Left => self.selected_date.checked_sub_days(Days::new(1)),
            KeyCode::Right => self.selected_date.checked_add_days(Days::new(1)),
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
