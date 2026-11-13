use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, layout::Rect, widgets::Paragraph};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, View, ViewAction, Workspace, WorkspaceError};

struct TestView {
    descriptor: ViewDescriptor,
}

impl TestView {
    fn new(id: &str, title: &str) -> Self {
        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new(id).unwrap(),
                title,
                "test view contribution",
            ),
        }
    }
}

impl View for TestView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.descriptor.title()), area);
    }

    fn render_inspector(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) -> bool {
        frame.render_widget(Paragraph::new("inspector contribution"), area);
        true
    }

    fn render_drawer(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) -> bool {
        frame.render_widget(Paragraph::new("drawer contribution"), area);
        true
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn control_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

fn workspace_with_two_views() -> Workspace {
    let mut workspace = Workspace::new();
    workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();
    workspace
        .register_view(TestView::new("activity", "Activity"))
        .unwrap();
    workspace
}

struct MinimalView {
    descriptor: ViewDescriptor,
}

impl View for MinimalView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new("minimal contribution"), area);
    }
}

struct ActionView {
    descriptor: ViewDescriptor,
    activations: std::rc::Rc<std::cell::Cell<usize>>,
}

impl View for ActionView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new("action view"), area);
    }

    fn command_palette_actions(&self) -> Vec<ViewAction> {
        vec![ViewAction::new(
            "create",
            "Capture item",
            "Create a new item quickly",
        )]
    }

    fn activate_command_palette_action(&mut self, id: &str) -> bool {
        if id != "create" {
            return false;
        }
        self.activations.set(self.activations.get() + 1);
        true
    }
}

#[test]
fn registered_views_navigate_and_can_be_removed() {
    let mut workspace = workspace_with_two_views();
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");

    workspace.handle_key(key(KeyCode::Right));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    assert_eq!(workspace.view_count(), 2);

    let removed = workspace.remove_view(&ViewId::new("activity").unwrap());
    assert!(removed.is_some());
    assert_eq!(workspace.view_count(), 1);
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");
}

#[test]
fn duplicate_view_registration_is_rejected() {
    let mut workspace = Workspace::new();
    workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();

    assert!(matches!(
        workspace.register_view(TestView::new("overview", "Duplicate")),
        Err(WorkspaceError::DuplicateView(_))
    ));
    assert_eq!(workspace.view_count(), 1);
}

#[test]
fn focus_panels_and_help_overlay_follow_keyboard_input() {
    let mut workspace = workspace_with_two_views();
    workspace.toggle_inspector();
    assert_eq!(workspace.focus(), FocusArea::Inspector);
    workspace.toggle_drawer();
    assert_eq!(workspace.focus(), FocusArea::Drawer);

    workspace.handle_key(key(KeyCode::Char('?')));
    assert!(workspace.help_is_open());
    workspace.handle_key(key(KeyCode::Esc));
    assert!(!workspace.help_is_open());

    workspace.handle_key(key(KeyCode::Char('?')));
    workspace.handle_key(key(KeyCode::Char('q')));
    assert!(workspace.should_quit());
}

#[test]
fn workspace_renders_on_wide_and_compact_terminals() {
    let mut workspace = workspace_with_two_views();
    for (width, height) in [(120, 36), (40, 10)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| workspace.render(frame)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Overview"));
        assert!(text.contains("tuilane"));
    }
}

#[test]
fn optional_panels_render_registered_view_contributions() {
    let mut workspace = workspace_with_two_views();
    workspace.toggle_inspector();
    workspace.toggle_drawer();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    assert!(text.contains("Inspector"));
    assert!(text.contains("inspector contribution"));
    assert!(text.contains("Drawer"));
    assert!(text.contains("drawer contribution"));
}

#[test]
fn shell_provides_fallbacks_for_missing_panel_contributions() {
    let mut workspace = Workspace::new();
    workspace
        .register_view(MinimalView {
            descriptor: ViewDescriptor::new(
                ViewId::new("minimal").unwrap(),
                "Minimal",
                "A view without optional panels",
            ),
        })
        .unwrap();
    workspace.toggle_inspector();
    workspace.toggle_drawer();
    workspace.handle_key(key(KeyCode::Char('?')));
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    assert!(text.contains("No contextual details for"));
    assert!(text.contains("this view."));
    assert!(text.contains("No supplemental content for this view."));
    assert!(text.contains("Workspace keys"));
}

#[test]
fn keyboard_focus_moves_only_through_visible_panels() {
    let mut workspace = workspace_with_two_views();
    let backend = TestBackend::new(120, 36);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    workspace.handle_key(key(KeyCode::Tab));
    assert_eq!(workspace.focus(), FocusArea::Navigation);
    workspace.handle_key(key(KeyCode::Down));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
}

#[test]
fn focus_returns_to_content_when_resize_hides_the_active_panel() {
    let mut workspace = workspace_with_two_views();
    workspace.toggle_inspector();
    assert_eq!(workspace.focus(), FocusArea::Inspector);

    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    assert_eq!(workspace.focus(), FocusArea::Content);
}

#[test]
fn command_palette_filters_and_switches_to_registered_views() {
    let mut workspace = workspace_with_two_views();
    workspace.handle_key(control_key(KeyCode::Char('p')));
    assert!(workspace.command_palette_is_open());

    for character in "activity".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));

    assert!(!workspace.command_palette_is_open());
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    assert!(!workspace.should_quit());
}

#[test]
fn command_palette_activates_view_actions_and_selects_their_owner() {
    let mut workspace = Workspace::new();
    workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();
    let activations = std::rc::Rc::new(std::cell::Cell::new(0));
    workspace
        .register_view(ActionView {
            descriptor: ViewDescriptor::new(
                ViewId::new("capture").unwrap(),
                "Capture",
                "Quickly add a record",
            ),
            activations: std::rc::Rc::clone(&activations),
        })
        .unwrap();

    workspace.handle_key(control_key(KeyCode::Char('p')));
    for character in "new item quickly".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));

    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "capture");
    assert_eq!(workspace.focus(), FocusArea::Content);
    assert_eq!(activations.get(), 1);
    assert!(!workspace.command_palette_is_open());
}

#[test]
fn command_palette_runs_shell_actions_and_traps_keys() {
    let mut workspace = workspace_with_two_views();
    workspace.handle_key(control_key(KeyCode::Char('p')));
    workspace.handle_key(key(KeyCode::Char('q')));
    assert!(!workspace.should_quit());
    workspace.handle_key(key(KeyCode::Esc));
    assert!(!workspace.command_palette_is_open());

    workspace.handle_key(control_key(KeyCode::Char('p')));
    workspace.handle_key(key(KeyCode::Down));
    workspace.handle_key(key(KeyCode::Down));
    workspace.handle_key(key(KeyCode::Enter));

    assert!(!workspace.command_palette_is_open());
    assert_eq!(workspace.focus(), FocusArea::Inspector);
}

#[test]
fn command_palette_overlay_renders_on_compact_terminals() {
    let mut workspace = workspace_with_two_views();
    workspace.handle_key(control_key(KeyCode::Char('p')));
    for _ in 0..5 {
        workspace.handle_key(key(KeyCode::Down));
    }
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();

    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("Commands"));
    assert!(text.contains("Quit workspace"));
}
