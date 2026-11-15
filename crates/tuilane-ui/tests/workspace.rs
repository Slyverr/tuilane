use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color, widgets::Paragraph};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{
    FocusArea, Keybinding, KeymapPreset, LayoutPreset, ThemePreset, View, ViewAction, Workspace,
    WorkspaceConfig, WorkspaceError,
};

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

    fn keybindings(&self, focus: FocusArea) -> Vec<Keybinding> {
        if focus == FocusArea::Content {
            vec![
                Keybinding::new("Create item", "c").with_priority(110),
                Keybinding::new("Rename item", "e").with_priority(10),
            ]
        } else {
            Vec::new()
        }
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

fn shift_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
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

struct ListView {
    descriptor: ViewDescriptor,
    selected: std::rc::Rc<std::cell::Cell<i32>>,
}

impl View for ListView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, _frame: &mut ratatui::Frame<'_>, _area: Rect) {}

    fn handle_key(&mut self, key: KeyEvent, focus: FocusArea) -> bool {
        if focus != FocusArea::Content {
            return false;
        }
        match key.code {
            KeyCode::Up => {
                self.selected.set(self.selected.get() - 1);
                true
            }
            KeyCode::Down => {
                self.selected.set(self.selected.get() + 1);
                true
            }
            _ => false,
        }
    }
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

    workspace.handle_key(shift_key(KeyCode::Char('l')));
    assert_eq!(workspace.focus(), FocusArea::Content);
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    assert_eq!(workspace.view_count(), 2);

    let removed = workspace.remove_view(&ViewId::new("activity").unwrap());
    assert!(removed.is_some());
    assert_eq!(workspace.view_count(), 1);
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");
}

#[test]
fn moving_between_focused_tabs_activates_each_view_immediately() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    workspace.handle_key(key(KeyCode::Tab));
    assert_eq!(workspace.focus(), FocusArea::Navigation);

    workspace.handle_key(key(KeyCode::Char('j')));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    workspace.handle_key(key(KeyCode::Char('c')));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    workspace.handle_key(key(KeyCode::Char('k')));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");
    workspace.handle_key(key(KeyCode::Char('l')));
    assert_eq!(workspace.focus(), FocusArea::Content);
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");

    workspace.handle_key(key(KeyCode::Tab));
    workspace.handle_key(key(KeyCode::Enter));
    assert_eq!(workspace.focus(), FocusArea::Content);
}

#[test]
fn clicking_a_tab_activates_its_view() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    assert!(workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 14, 0,)));

    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    assert_eq!(workspace.focus(), FocusArea::Content);
}

#[test]
fn scrolling_over_tabs_activates_the_new_view() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    assert!(workspace.handle_mouse(mouse(MouseEventKind::ScrollDown, 5, 0)));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    assert_eq!(workspace.focus(), FocusArea::Content);
}

#[test]
fn alt_tab_opens_a_selector_that_cycles_and_activates_views() {
    let mut workspace = workspace_with_two_views();
    let alt_tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::ALT);
    workspace.handle_key(alt_tab);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let popup = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(popup.contains("Switch view"));
    assert!(popup.contains("Overview"));
    assert!(popup.contains("Activity"));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");

    workspace.handle_key(key(KeyCode::Down));
    workspace.handle_key(key(KeyCode::Up));
    workspace.handle_key(key(KeyCode::Enter));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    assert_eq!(workspace.focus(), FocusArea::Content);
}

#[test]
fn dismissing_alt_tab_picker_keeps_the_current_view_active() {
    let mut workspace = workspace_with_two_views();
    workspace.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::ALT));
    workspace.handle_key(key(KeyCode::Esc));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");
}

#[test]
fn clicking_a_view_in_the_alt_tab_picker_activates_it() {
    let mut workspace = workspace_with_two_views();
    workspace.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::ALT));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    assert!(workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 15, 12)));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
}

#[test]
fn clicking_visible_panels_moves_keyboard_focus() {
    let mut workspace = workspace_with_two_views();
    workspace.toggle_inspector();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    assert!(workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 45, 5,)));
    assert_eq!(workspace.focus(), FocusArea::Content);

    assert!(workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 100, 5,)));
    assert_eq!(workspace.focus(), FocusArea::Inspector);
}

#[test]
fn j_and_k_in_content_do_not_switch_tabs() {
    let mut workspace = workspace_with_two_views();

    workspace.handle_key(key(KeyCode::Char('j')));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");
    workspace.handle_key(key(KeyCode::Char('k')));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");
}

#[test]
fn j_and_k_are_forwarded_to_the_active_views_vertical_navigation() {
    let selected = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut workspace = Workspace::new();
    workspace
        .register_view(ListView {
            descriptor: ViewDescriptor::new(
                ViewId::new("list").unwrap(),
                "List",
                "A view with vertical navigation",
            ),
            selected: std::rc::Rc::clone(&selected),
        })
        .unwrap();

    workspace.handle_key(key(KeyCode::Char('j')));
    workspace.handle_key(key(KeyCode::Char('j')));
    workspace.handle_key(key(KeyCode::Char('k')));

    assert_eq!(selected.get(), 1);
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "list");
    assert_eq!(workspace.focus(), FocusArea::Content);
}

#[test]
fn focus_layout_hides_navigation_and_high_contrast_marks_selection() {
    let mut focus_workspace = Workspace::with_config(WorkspaceConfig::new(
        ThemePreset::Default,
        LayoutPreset::Focus,
        KeymapPreset::Default,
    ));
    focus_workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();
    let mut focus_terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    focus_terminal
        .draw(|frame| focus_workspace.render(frame))
        .unwrap();
    let focus_text = focus_terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(focus_text.contains("◦ Overview"));
    assert!(focus_text.contains("Overview"));

    let mut themed_workspace = Workspace::with_config(WorkspaceConfig::new(
        ThemePreset::HighContrast,
        LayoutPreset::Balanced,
        KeymapPreset::Default,
    ));
    themed_workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();
    let mut themed_terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    themed_terminal
        .draw(|frame| themed_workspace.render(frame))
        .unwrap();
    let selected_label = &themed_terminal.backend().buffer()[(3, 0)];
    assert_eq!(selected_label.symbol(), "O");
    assert_eq!(selected_label.fg, Color::Yellow);
}

#[test]
fn tabs_render_icons_and_mark_the_immediately_active_view() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    let buffer = terminal.backend().buffer();
    let text = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("◦ Overview"));
    assert!(text.contains("◦ Activity"));
    assert!(!text.contains("Views"));

    workspace.handle_key(shift_key(KeyCode::Char('l')));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(14, 0)].fg, Color::Cyan);
    let content = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(content.contains("Activity"));
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
        assert!(text.contains("◦ Overview"));
    }
}

#[test]
fn footer_and_help_show_view_owned_shortcuts() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    let buffer = terminal.backend().buffer();
    let text = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("[c] Create item"));

    workspace.handle_key(key(KeyCode::Char('?')));
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let help = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(help.contains("Overview actions"));
    assert!(help.contains("Workspace"));
    assert!(help.contains("Mouse"));
    assert!(help.contains("Switch tabs"));
    assert!(!help.contains("No view shortcuts"));
}

#[test]
fn footer_keeps_one_line_and_prioritizes_view_actions() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let footer = (0..40)
        .map(|x| terminal.backend().buffer()[(x, 11)].symbol())
        .collect::<String>();
    assert!(footer.contains("[c] Create item"), "footer: {footer}");
    assert!(footer.contains("Help +"), "footer: {footer}");
    assert!(!footer.contains("Rename item"), "footer: {footer}");
}

#[test]
fn panel_titles_are_separated_from_the_top_border() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 1)].symbol(), " ");
    assert_eq!(buffer[(2, 1)].symbol(), "O");
}

#[test]
fn optional_panels_render_registered_view_contributions() {
    let mut workspace = workspace_with_two_views();
    workspace.toggle_inspector();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let inspector = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(inspector.contains("Inspector"));
    assert!(inspector.contains("inspector contribution"));

    workspace.toggle_drawer();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let drawer = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(drawer.contains("Drawer"));
    assert!(drawer.contains("drawer contribution"));
    assert!(!drawer.contains("inspector contribution"));
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
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let inspector = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        inspector.contains("No contextual details"),
        "unexpected inspector output: {inspector}"
    );
    workspace.toggle_drawer();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let drawer = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(drawer.contains("No supplemental content"));

    workspace.handle_key(key(KeyCode::Char('?')));
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let help = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(help.contains("Switch tabs"));
    assert!(!help.contains("Resize Views"));
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
    workspace.handle_key(key(KeyCode::Enter));
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
