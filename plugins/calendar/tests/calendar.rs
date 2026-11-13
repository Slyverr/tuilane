use std::{
    cell::{Cell, RefCell},
    convert::Infallible,
    rc::Rc,
};

use chrono::{Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_calendar::CalendarView;
use tuilane_core::{EventBus, ServiceBroker};
use tuilane_plugin_api::tasks::{
    Task, TaskId, TaskStatus, TaskSummary, TaskTitle, TasksByDateV1, TasksChangedV1,
};
use tuilane_plugin_api::{CapabilityId, ServiceRequest};
use tuilane_ui::{FocusArea, View};

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid test date")
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn calendar(today: NaiveDate) -> CalendarView {
    let events = EventBus::new();
    CalendarView::new(today, Rc::new(RefCell::new(ServiceBroker::new())), &events)
}

fn render_inspector(view: &mut CalendarView) -> String {
    let mut terminal = Terminal::new(TestBackend::new(64, 16)).expect("create test terminal");
    terminal
        .draw(|frame| {
            view.render_inspector(frame, frame.area());
        })
        .expect("render calendar inspector");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

fn summary(title: &str) -> TaskSummary {
    let task = Task::new(
        TaskId::new("00000000-0000-4000-8000-000000000001").expect("valid task id"),
        TaskTitle::new(title).expect("valid task title"),
        None,
        TaskStatus::Open,
        "2024-02-01T00:00:00Z",
        "2024-02-01T00:00:00Z",
    );
    TaskSummary::from_task(&task)
}

#[test]
fn month_navigation_clamps_to_the_last_valid_day() {
    let mut calendar = calendar(date(2024, 3, 31));

    assert!(calendar.handle_key(key(KeyCode::PageDown), FocusArea::Content));

    assert_eq!(calendar.selected_date(), date(2024, 4, 30));
}

#[test]
fn day_navigation_includes_leap_day() {
    let mut calendar = calendar(date(2024, 2, 28));

    assert!(calendar.handle_key(key(KeyCode::Right), FocusArea::Content));

    assert_eq!(calendar.selected_date(), date(2024, 2, 29));
}

#[test]
fn today_key_selects_the_current_local_date() {
    let mut calendar = calendar(date(2020, 1, 1));

    assert!(calendar.handle_key(key(KeyCode::Char('t')), FocusArea::Content));

    assert_eq!(calendar.selected_date(), Local::now().date_naive());
}

#[test]
fn calendar_does_not_consume_date_keys_outside_content_focus() {
    let mut calendar = calendar(date(2024, 2, 28));

    assert!(!calendar.handle_key(key(KeyCode::Right), FocusArea::Inspector));
}

#[test]
fn vi_navigation_keys_remain_available_to_the_workspace_shell() {
    let mut calendar = calendar(date(2024, 2, 28));

    assert!(!calendar.handle_key(key(KeyCode::Char('h')), FocusArea::Content));
}

#[test]
fn calendar_renders_the_selected_month_and_date() {
    let mut calendar = calendar(date(2024, 2, 29));
    let mut terminal = Terminal::new(TestBackend::new(48, 14)).expect("create test terminal");
    terminal
        .draw(|frame| calendar.render(frame, frame.area()))
        .expect("render calendar");
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();

    assert!(text.contains("February 2024"));
    assert!(text.contains("[29]"));
    assert!(text.contains("Selected date: 2024-02-29"));
}

#[test]
fn inspector_queries_the_selected_date_through_the_service_broker() {
    let requested_dates = Rc::new(RefCell::new(Vec::new()));
    let observed_dates = Rc::clone(&requested_dates);
    let mut services = ServiceBroker::new();
    services
        .register(move |request: TasksByDateV1| {
            observed_dates
                .borrow_mut()
                .push(request.date().as_str().to_owned());
            Ok::<_, Infallible>(vec![summary("Prepare demo")])
        })
        .expect("register date-query service");
    let events = EventBus::new();
    let mut calendar =
        CalendarView::new(date(2024, 2, 28), Rc::new(RefCell::new(services)), &events);
    calendar.handle_key(key(KeyCode::Right), FocusArea::Content);

    let rendered = render_inspector(&mut calendar);

    assert_eq!(requested_dates.borrow().as_slice(), ["2024-02-29"]);
    assert!(rendered.contains("Prepare demo (open)"));
}

#[test]
fn missing_tasks_service_does_not_disable_calendar_navigation() {
    let mut calendar = calendar(date(2024, 2, 28));

    let rendered = render_inspector(&mut calendar);
    calendar.handle_key(key(KeyCode::Right), FocusArea::Content);

    assert!(rendered.contains("Tasks service is unavailable."));
    assert_eq!(calendar.selected_date(), date(2024, 2, 29));
}

struct TasksByDateV2;

impl ServiceRequest for TasksByDateV2 {
    type Response = Vec<TaskSummary>;

    const CAPABILITY: CapabilityId = CapabilityId::new("tasks.query.by_date", 2);
}

#[test]
fn incompatible_tasks_service_version_is_reported_in_the_inspector() {
    let mut services = ServiceBroker::new();
    services
        .register(|_: TasksByDateV2| Ok::<_, Infallible>(Vec::new()))
        .expect("register incompatible service version");
    let events = EventBus::new();
    let mut calendar =
        CalendarView::new(date(2024, 2, 28), Rc::new(RefCell::new(services)), &events);

    let rendered = render_inspector(&mut calendar);

    assert!(rendered.contains("Tasks service version is incompatible."));
}

#[test]
fn task_change_events_invalidate_the_inspector_query() {
    let query_count = Rc::new(Cell::new(0));
    let observed_count = Rc::clone(&query_count);
    let mut services = ServiceBroker::new();
    services
        .register(move |_: TasksByDateV1| {
            observed_count.set(observed_count.get() + 1);
            Ok::<_, Infallible>(Vec::new())
        })
        .expect("register date-query service");
    let events = EventBus::new();
    let mut calendar =
        CalendarView::new(date(2024, 2, 28), Rc::new(RefCell::new(services)), &events);

    render_inspector(&mut calendar);
    render_inspector(&mut calendar);
    assert_eq!(query_count.get(), 1);

    events.publish(TasksChangedV1);
    render_inspector(&mut calendar);

    assert_eq!(query_count.get(), 2);
}
