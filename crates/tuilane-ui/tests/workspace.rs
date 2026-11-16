use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Terminal, backend::TestBackend, layout::Rect, style::Color, text::Line, widgets::Paragraph,
};
use tuilane_plugin_api::{
    DashboardItem, DashboardLayout, DashboardOption, DashboardOptionKind, DashboardSize,
    DashboardWidget, DashboardWidgetContent, DashboardWidgetDescriptor, DashboardWidgetId,
    ViewDescriptor, ViewId,
};
use tuilane_ui::{
    FocusArea, Keybinding, KeymapPreset, LayoutPreset, NavigationStyle, ThemePreset, View,
    ViewAction, ViewDialog, Workspace, WorkspaceConfig, WorkspaceError, icons, render_empty_state,
};

struct TestView {
    descriptor: ViewDescriptor,
}

struct OverlayView {
    descriptor: ViewDescriptor,
    dialog_open: bool,
    notification: Option<String>,
}

impl View for OverlayView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new("underlying content"), area);
    }

    fn handle_key(&mut self, key: KeyEvent, _focus: FocusArea) -> bool {
        match key.code {
            KeyCode::Char('d') => self.dialog_open = true,
            KeyCode::Char('n') => self.notification = Some("Saved item".to_owned()),
            KeyCode::Esc => self.dialog_open = false,
            _ => return false,
        }
        true
    }

    fn keybindings(&self, _focus: FocusArea) -> Vec<Keybinding> {
        if self.dialog_open {
            vec![
                Keybinding::new("Save", "Enter"),
                Keybinding::new("Cancel", "Esc"),
            ]
        } else {
            Vec::new()
        }
    }

    fn dialog(&self) -> Option<ViewDialog> {
        self.dialog_open.then(|| ViewDialog::Input {
            title: "New entry".to_owned(),
            label: "Name".to_owned(),
            value: "initial".to_owned(),
            error: None,
        })
    }

    fn take_notification(&mut self) -> Option<String> {
        self.notification.take()
    }
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
                Keybinding::new("Create", "c").with_priority(110),
                Keybinding::new("Rename", "e").with_priority(10),
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

fn alt_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
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

fn workspace_with_three_views() -> Workspace {
    let mut workspace = workspace_with_two_views();
    workspace
        .register_view(TestView::new("calendar", "Calendar"))
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

struct DashboardShellView {
    descriptor: ViewDescriptor,
}

struct DashboardWidgetView {
    descriptor: ViewDescriptor,
    opened: std::rc::Rc<std::cell::Cell<bool>>,
}

struct MouseView {
    descriptor: ViewDescriptor,
    clicks: std::rc::Rc<std::cell::Cell<usize>>,
}

impl View for MouseView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new("mouse content"), area);
    }

    fn handle_mouse(&mut self, event: MouseEvent, focus: FocusArea) -> bool {
        if focus == FocusArea::Content && event.kind == MouseEventKind::Down(MouseButton::Left) {
            self.clicks.set(self.clicks.get() + 1);
            true
        } else {
            false
        }
    }
}

impl DashboardShellView {
    fn new() -> Self {
        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("overview").unwrap(),
                "Overview",
                "Workspace dashboard",
            ),
        }
    }
}

impl View for DashboardShellView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, _frame: &mut ratatui::Frame<'_>, _area: Rect) {}

    fn is_dashboard(&self) -> bool {
        true
    }
}

impl View for DashboardWidgetView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new("widget owner view"), area);
    }

    fn dashboard_widgets(&mut self, _layout: &DashboardLayout) -> Vec<DashboardWidget> {
        let id = DashboardWidgetId::new("org.example.activity.queue").unwrap();
        vec![DashboardWidget::new(
            DashboardWidgetDescriptor::new(
                id,
                "Build queue",
                "Recent work waiting to be built",
                DashboardSize::Compact,
                vec![DashboardOption::new(
                    "limit",
                    "Visible items",
                    DashboardOptionKind::Number {
                        default: 3,
                        min: 1,
                        max: 8,
                    },
                )],
            ),
            DashboardWidgetContent::List {
                items: vec![DashboardItem::new("Ship release", None)],
                empty_message: "Nothing queued".into(),
            },
        )]
    }

    fn open_dashboard_widget(&mut self, id: &DashboardWidgetId) {
        if id.as_str() == "org.example.activity.queue" {
            self.opened.set(true);
        }
    }
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
fn overview_renders_plugin_widgets_and_opens_their_owner() {
    let opened = std::rc::Rc::new(std::cell::Cell::new(false));
    let mut workspace = Workspace::new();
    workspace.register_view(DashboardShellView::new()).unwrap();
    workspace
        .register_view(DashboardWidgetView {
            descriptor: ViewDescriptor::new(
                ViewId::new("activity").unwrap(),
                "Activity",
                "Recent activity",
            ),
            opened: opened.clone(),
        })
        .unwrap();

    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(screen.contains("Build queue"));
    assert!(screen.contains("Ship release"));

    workspace.handle_key(key(KeyCode::Enter));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    assert!(opened.get());
}

#[test]
fn overview_editor_persists_widget_size_visibility_and_options() {
    let mut workspace = Workspace::new();
    workspace.register_view(DashboardShellView::new()).unwrap();
    workspace
        .register_view(DashboardWidgetView {
            descriptor: ViewDescriptor::new(
                ViewId::new("activity").unwrap(),
                "Activity",
                "Recent activity",
            ),
            opened: std::rc::Rc::new(std::cell::Cell::new(false)),
        })
        .unwrap();

    workspace.handle_key(key(KeyCode::Char('e')));
    workspace.handle_key(key(KeyCode::Char('+')));
    workspace.handle_key(key(KeyCode::Char('o')));
    workspace.handle_key(key(KeyCode::Right));
    workspace.handle_key(key(KeyCode::Esc));
    workspace.handle_key(key(KeyCode::Char(' ')));

    let id = DashboardWidgetId::new("org.example.activity.queue").unwrap();
    let preference = workspace
        .config()
        .dashboard_layout()
        .preference(&id)
        .unwrap();
    assert_eq!(preference.size(), DashboardSize::Wide);
    assert_eq!(
        preference.option("limit"),
        Some(&tuilane_plugin_api::DashboardOptionValue::Number(4))
    );
    assert!(!preference.visible());
}

#[test]
fn overview_editor_supports_mouse_visibility_size_and_option_controls() {
    let mut workspace = Workspace::new();
    workspace.register_view(DashboardShellView::new()).unwrap();
    workspace
        .register_view(DashboardWidgetView {
            descriptor: ViewDescriptor::new(
                ViewId::new("activity").unwrap(),
                "Activity",
                "Recent activity",
            ),
            opened: std::rc::Rc::new(std::cell::Cell::new(false)),
        })
        .unwrap();
    workspace.handle_key(key(KeyCode::Char('e')));

    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let row = (0..32)
        .find(|row| {
            let line = (0..120)
                .map(|column| terminal.backend().buffer()[(column, *row)].symbol())
                .collect::<String>();
            line.contains("Build queue") && line.contains("[Options]")
        })
        .unwrap();

    workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 24, row));
    workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 90, row));
    workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 78, row));
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let option_row = (0..32)
        .find(|row| {
            (0..120)
                .map(|column| terminal.backend().buffer()[(column, *row)].symbol())
                .collect::<String>()
                .contains("Visible items")
        })
        .unwrap();
    workspace.handle_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        96,
        option_row,
    ));

    let id = DashboardWidgetId::new("org.example.activity.queue").unwrap();
    let preference = workspace
        .config()
        .dashboard_layout()
        .preference(&id)
        .unwrap();
    assert!(!preference.visible());
    assert_eq!(preference.size(), DashboardSize::Wide);
    assert_eq!(
        preference.option("limit"),
        Some(&tuilane_plugin_api::DashboardOptionValue::Number(4))
    );
}

#[test]
fn moving_between_focused_tabs_activates_each_view_immediately() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 100, 0));
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

    assert!(workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 15, 0,)));

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
    assert!(popup.contains("Views"));
    assert!(popup.contains("Overview"));
    assert!(popup.contains("Activity"));
    assert!(popup.contains("2/2"));
    assert!(popup.contains("test view contribution"));
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

    assert!(workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 15, 11)));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
}

#[test]
fn tab_picker_filters_views_and_backspace_edits_the_query() {
    let mut workspace = workspace_with_three_views();
    workspace.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::ALT));
    for character in "cal".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let calendar_row = (0..80)
        .map(|x| terminal.backend().buffer()[(x, 11)].symbol())
        .collect::<String>();
    assert!(calendar_row.contains("Calendar"), "row: {calendar_row}");

    workspace.handle_key(key(KeyCode::Char('d')));
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let empty_row = (0..80)
        .map(|x| terminal.backend().buffer()[(x, 11)].symbol())
        .collect::<String>();
    assert!(empty_row.contains("No matching views"), "row: {empty_row}");
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");

    workspace.handle_key(key(KeyCode::Backspace));
    workspace.handle_key(key(KeyCode::Enter));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "calendar");
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
fn workspace_routes_content_clicks_to_the_active_view() {
    let clicks = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut workspace = Workspace::new();
    workspace
        .register_view(MouseView {
            descriptor: ViewDescriptor::new(
                ViewId::new("mouse").unwrap(),
                "Mouse",
                "View that accepts pointer input",
            ),
            clicks: clicks.clone(),
        })
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    assert!(workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 10, 5)));
    assert_eq!(clicks.get(), 1);
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
    assert!(focus_text.contains(&format!("{}  Overview", icons::DEFAULT)));
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
    let selected_label = &themed_terminal.backend().buffer()[(5, 0)];
    assert_eq!(selected_label.symbol(), "O");
    assert_eq!(selected_label.bg, Color::Yellow);
}

#[test]
fn tabs_render_icons_and_mark_the_immediately_active_view() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(2, 0)].symbol(), icons::DEFAULT);
    assert_eq!(buffer[(1, 0)].symbol(), " ");
    assert_eq!(buffer[(3, 0)].symbol(), " ");
    assert_eq!(buffer[(4, 0)].symbol(), " ");
    assert_eq!(buffer[(5, 0)].symbol(), "O");
    assert_eq!(buffer[(17, 0)].symbol(), icons::DEFAULT);
    assert_eq!(buffer[(17, 0)].fg, Color::Cyan);
    let text = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains(&format!("{}  Overview", icons::DEFAULT)));
    assert!(text.contains(&format!("{}  Activity", icons::DEFAULT)));
    assert!(!text.contains("Views"));

    workspace.handle_key(shift_key(KeyCode::Char('l')));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(15, 0)].bg, Color::Cyan);
    let content = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(content.contains("Activity"));
}

#[test]
fn built_in_icons_fit_one_terminal_cell() {
    let icons = [
        icons::DEFAULT,
        icons::OVERVIEW,
        icons::TASKS,
        icons::TASK_OPEN,
        icons::TASK_DONE,
        icons::CALENDAR,
        icons::PROJECTS,
        icons::NOTES,
        icons::IDEAS,
        icons::LINKS,
        icons::SEARCH,
        icons::ASSISTANT,
        icons::COMMANDS,
        icons::STORAGE,
        icons::SERVICES,
        icons::EVENTS,
        icons::VIEWS,
        icons::PERMISSIONS,
        icons::INSPECTOR,
        icons::DRAWER,
        icons::NAVIGATION,
        icons::COLLAPSE,
        icons::EXPAND,
        icons::HELP,
        icons::QUIT,
    ];

    for icon in icons {
        assert_eq!(Line::from(icon).width(), 1, "icon {icon:?}");
    }
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
        assert!(text.contains(&format!("{}  Overview", icons::DEFAULT)));
    }
}

#[test]
fn empty_state_centers_the_title_and_next_step() {
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    terminal
        .draw(|frame| {
            render_empty_state(
                frame,
                frame.area(),
                "No notes yet",
                "Create a note to begin.",
            );
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let title_row = (0..40).map(|x| buffer[(x, 4)].symbol()).collect::<String>();
    let detail_row = (0..40).map(|x| buffer[(x, 5)].symbol()).collect::<String>();
    assert!(title_row.contains("No notes yet"), "row: {title_row}");
    assert!(
        detail_row.contains("Create a note to begin."),
        "row: {detail_row}"
    );
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
    assert!(text.contains("[c] Create"));

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
    assert!(help.contains("Switch"));
    assert!(!help.contains("No view shortcuts"));
}

#[test]
fn footer_keeps_one_line_and_prioritizes_view_actions() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(26, 12)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let footer = (0..26)
        .map(|x| terminal.backend().buffer()[(x, 11)].symbol())
        .collect::<String>();
    assert!(footer.contains("[c] Create"), "footer: {footer}");
    assert!(footer.contains("Help +"), "footer: {footer}");
    assert!(!footer.contains("[e] Rename"), "footer: {footer}");
    assert!(!footer.contains("[Alt+Tab]"), "footer: {footer}");
    assert!(!footer.contains("[Shift+H/L]"), "footer: {footer}");
}

#[test]
fn view_title_appears_in_the_tab_without_a_duplicate_panel_title() {
    let mut workspace = workspace_with_two_views();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let tabs = (0..80).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
    let panel_top = (0..80).map(|x| buffer[(x, 1)].symbol()).collect::<String>();
    assert!(tabs.contains("Overview"), "tabs: {tabs}");
    assert!(!panel_top.contains("Overview"), "panel top: {panel_top}");
}

#[test]
fn icon_only_tabs_hide_labels_and_keep_the_selected_view_usable() {
    let config = WorkspaceConfig::default().with_navigation(NavigationStyle::Tabs, true);
    let mut workspace = Workspace::with_config(config);
    workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();
    workspace
        .register_view(TestView::new("activity", "Activity"))
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let tabs = (0..80)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
        .collect::<String>();
    let content_border = (0..80)
        .map(|x| terminal.backend().buffer()[(x, 1)].symbol())
        .collect::<String>();
    assert!(!tabs.contains("Overview"), "tabs: {tabs}");
    assert!(!tabs.contains("Activity"), "tabs: {tabs}");
    assert!(
        content_border.contains("Overview"),
        "content: {content_border}"
    );
}

#[test]
fn collapsed_sidebar_uses_glyph_rows_and_mouse_activation() {
    let config = WorkspaceConfig::default().with_navigation(NavigationStyle::Sidebar, true);
    let mut workspace = Workspace::with_config(config);
    workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();
    workspace
        .register_view(TestView::new("activity", "Activity"))
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let before = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(!before.contains("Activity"), "screen: {before}");

    workspace.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, 4));
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let after = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(after.contains("Activity"), "screen: {after}");
}

#[test]
fn expanded_sidebar_highlights_each_three_line_navigation_entry() {
    let config = WorkspaceConfig::default().with_navigation(NavigationStyle::Sidebar, false);
    let mut workspace = Workspace::with_config(config);
    workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();
    workspace
        .register_view(TestView::new("activity", "Activity"))
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    for y in 0..3 {
        for x in 0..24 {
            assert_eq!(buffer[(x, y)].bg, Color::Cyan, "cell ({x}, {y})");
        }
    }
    assert_eq!(buffer[(0, 3)].bg, Color::Reset);
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
    assert!(help.contains("Switch"));
    assert!(!help.contains("Resize Views"));
}

#[test]
fn bare_tab_keys_do_not_change_focus_or_switch_views() {
    let mut workspace = workspace_with_two_views();
    workspace.handle_key(key(KeyCode::Tab));
    workspace.handle_key(key(KeyCode::BackTab));

    assert_eq!(workspace.focus(), FocusArea::Content);
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "overview");
}

#[test]
fn view_dialog_is_rendered_as_a_centered_host_overlay() {
    let mut workspace = Workspace::new();
    workspace
        .register_view(OverlayView {
            descriptor: ViewDescriptor::new(
                ViewId::new("overlay").unwrap(),
                "Overlay",
                "test dialog contribution",
            ),
            dialog_open: true,
            notification: None,
        })
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();

    let buffer = terminal.backend().buffer();
    let dialog_top = (0..120)
        .find(|x| buffer[(*x, 15)].symbol() == "╭")
        .expect("dialog should be centered horizontally");
    assert!((20..=40).contains(&dialog_top));
    let screen = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(screen.contains("New entry"));
    assert!(screen.contains("Name"));
    assert!(screen.contains("initial"));
    assert!(screen.contains("[Enter] Save"));
}

#[test]
fn view_notifications_render_in_the_shell_toast_area() {
    let mut workspace = Workspace::new();
    workspace
        .register_view(OverlayView {
            descriptor: ViewDescriptor::new(
                ViewId::new("notice").unwrap(),
                "Notice",
                "test notification contribution",
            ),
            dialog_open: false,
            notification: None,
        })
        .unwrap();
    workspace.handle_key(key(KeyCode::Char('n')));
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();

    let buffer = terminal.backend().buffer();
    let notice_row = (0..120)
        .map(|x| buffer[(x, 33)].symbol())
        .collect::<String>();
    let content_row = (0..120)
        .map(|x| buffer[(x, 2)].symbol())
        .collect::<String>();
    assert!(notice_row.contains("Saved item"));
    assert!(content_row.contains("underlying content"));
    assert!(!content_row.contains("Saved item"));
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
fn command_palette_switches_navigation_layout_and_collapse_state() {
    let mut workspace = workspace_with_two_views();

    workspace.handle_key(control_key(KeyCode::Char('p')));
    for character in "navigation sidebar".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    assert_eq!(workspace.config().navigation(), NavigationStyle::Sidebar);

    workspace.handle_key(control_key(KeyCode::Char('p')));
    for character in "icons".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    assert!(workspace.config().navigation_collapsed());
}

#[test]
fn sidebar_keyboard_navigation_preserves_view_actions_until_entered() {
    let config = WorkspaceConfig::default().with_navigation(NavigationStyle::Sidebar, false);
    let mut workspace = Workspace::with_config(config);
    workspace
        .register_view(TestView::new("overview", "Overview"))
        .unwrap();
    workspace
        .register_view(TestView::new("activity", "Activity"))
        .unwrap();
    workspace
        .register_view(TestView::new("calendar", "Calendar"))
        .unwrap();

    workspace.handle_key(alt_key(KeyCode::Left));
    assert_eq!(workspace.focus(), FocusArea::Navigation);
    workspace.handle_key(key(KeyCode::Char('j')));
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
    assert_eq!(workspace.focus(), FocusArea::Navigation);
    workspace.handle_key(key(KeyCode::Char('l')));
    assert_eq!(workspace.focus(), FocusArea::Content);
    assert_eq!(workspace.selected_view_id().unwrap().as_str(), "activity");
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

#[test]
fn command_palette_shows_result_context_and_description() {
    let mut workspace = workspace_with_two_views();
    workspace.handle_key(control_key(KeyCode::Char('p')));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    terminal.draw(|frame| workspace.render(frame)).unwrap();

    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("Overview"));
    assert!(text.contains("View"));
    assert!(text.contains("test view contribution"));
}
