use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_calendar::CalendarView;
use tuilane_ui::{FocusArea, View};

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid test date")
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn month_navigation_clamps_to_the_last_valid_day() {
    let mut calendar = CalendarView::new(date(2024, 3, 31));

    assert!(calendar.handle_key(key(KeyCode::PageDown), FocusArea::Content));

    assert_eq!(calendar.selected_date(), date(2024, 4, 30));
}

#[test]
fn day_navigation_includes_leap_day() {
    let mut calendar = CalendarView::new(date(2024, 2, 28));

    assert!(calendar.handle_key(key(KeyCode::Right), FocusArea::Content));

    assert_eq!(calendar.selected_date(), date(2024, 2, 29));
}

#[test]
fn calendar_does_not_consume_date_keys_outside_content_focus() {
    let mut calendar = CalendarView::new(date(2024, 2, 28));

    assert!(!calendar.handle_key(key(KeyCode::Right), FocusArea::Inspector));
}

#[test]
fn calendar_renders_the_selected_month_and_date() {
    let mut calendar = CalendarView::new(date(2024, 2, 29));
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
