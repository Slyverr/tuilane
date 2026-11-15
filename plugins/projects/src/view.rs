use std::{cell::Cell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
use tuilane_core::{CommandDispatcher, EventSubscription};
use tuilane_plugin_api::projects::{
    Project, ProjectCommand, ProjectCommandResult, ProjectId, ProjectTitle, ProjectsChangedV1,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{
    BrowserDetails, BrowserHitMap, BrowserItem, DashboardItem, DashboardLayout, DashboardOption,
    DashboardOptionKind, DashboardOptionValue, DashboardSize, DashboardTone, DashboardWidget,
    DashboardWidgetContent, DashboardWidgetDescriptor, DashboardWidgetId, FocusArea, Keybinding,
    ThemePreset, View, ViewAction, ViewDialog, render_browser, select_browser_row,
};

enum InputMode {
    Title {
        id: Option<ProjectId>,
        value: String,
    },
    ConfirmDelete(ProjectId),
}

/// Projects view that routes changes through the host command dispatcher.
pub struct ProjectsView {
    descriptor: ViewDescriptor,
    commands: Rc<std::cell::RefCell<CommandDispatcher>>,
    projects: Vec<Project>,
    selected: Option<usize>,
    input_mode: Option<InputMode>,
    status: Option<String>,
    load_error: Option<String>,
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
    browser_hits: BrowserHitMap,
}

impl ProjectsView {
    /// Creates a Projects view sharing the host's command and event paths.
    pub fn new(
        commands: Rc<std::cell::RefCell<CommandDispatcher>>,
        events: &tuilane_core::EventBus,
    ) -> Self {
        let dirty = Rc::new(Cell::new(true));
        let event_dirty = Rc::clone(&dirty);
        let event_subscription = events.subscribe(move |_: &ProjectsChangedV1| {
            event_dirty.set(true);
        });

        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("projects").expect("static view identifier is valid"),
                "Projects",
                "Group related work into durable projects",
            ),
            commands,
            projects: Vec::new(),
            selected: None,
            input_mode: None,
            status: None,
            load_error: None,
            dirty,
            _event_subscription: event_subscription,
            browser_hits: BrowserHitMap::default(),
        }
    }

    fn refresh(&mut self) {
        if !self.dirty.replace(false) {
            return;
        }

        let result = self
            .commands
            .try_borrow_mut()
            .map_err(|_| "command dispatcher is busy".to_owned())
            .and_then(|mut commands| {
                commands
                    .execute(ProjectCommand::List)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(ProjectCommandResult::Projects(projects)) => {
                self.load_error = None;
                self.projects = projects;
                self.selected = match (self.selected, self.projects.is_empty()) {
                    (_, true) => None,
                    (Some(index), false) => Some(index.min(self.projects.len() - 1)),
                    (None, false) => Some(0),
                };
            }
            Ok(_) => {
                let error = "Project list returned an unexpected response.".to_owned();
                self.status = Some(error.clone());
                self.load_error = Some(error);
            }
            Err(error) => {
                self.load_error = Some(error.clone());
                self.status = Some(error);
            }
        }
    }

    fn selected_project(&self) -> Option<&Project> {
        self.selected.and_then(|index| self.projects.get(index))
    }

    fn run_command(
        &mut self,
        command: ProjectCommand,
        success: impl FnOnce(&ProjectCommandResult) -> String,
    ) {
        let result = self
            .commands
            .try_borrow_mut()
            .map_err(|_| "command dispatcher is busy".to_owned())
            .and_then(|mut commands| commands.execute(command).map_err(|error| error.to_string()));
        match result {
            Ok(response) => {
                self.status = Some(success(&response));
                self.dirty.set(true);
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn finish_title_input(&mut self, id: Option<ProjectId>, value: String) {
        let title = match ProjectTitle::new(&value) {
            Ok(title) => title,
            Err(error) => {
                self.status = Some(error.to_string());
                self.input_mode = Some(InputMode::Title { id, value });
                return;
            }
        };

        match id {
            Some(id) => {
                self.run_command(
                    ProjectCommand::Rename { id, title },
                    |response| match response {
                        ProjectCommandResult::Project(project) => {
                            format!("Renamed to {}", project.title())
                        }
                        _ => "Updated project.".to_owned(),
                    },
                )
            }
            None => self.run_command(
                ProjectCommand::Create { title },
                |response| match response {
                    ProjectCommandResult::Project(project) => {
                        format!("Created {}", project.title())
                    }
                    _ => "Created project.".to_owned(),
                },
            ),
        }
    }

    fn move_selection(&mut self, offset: isize) {
        if self.projects.is_empty() {
            self.selected = None;
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        self.selected = Some((current + offset).rem_euclid(self.projects.len() as isize) as usize);
    }
}

impl View for ProjectsView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn icon(&self) -> &'static str {
        tuilane_ui::icons::PROJECTS
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.render_with_theme(frame, area, ThemePreset::default());
    }

    fn render_with_theme(&mut self, frame: &mut Frame<'_>, area: Rect, theme: ThemePreset) {
        self.refresh();
        let items = self
            .projects
            .iter()
            .map(|project| {
                let updated = project
                    .updated_at()
                    .get(..10)
                    .unwrap_or(project.updated_at());
                BrowserItem::new(project.title().as_str())
                    .with_icon(tuilane_ui::icons::PROJECTS)
                    .with_summary(format!("Updated {updated}"))
            })
            .collect::<Vec<_>>();
        let details = self.selected_project().map(|project| {
            BrowserDetails::new(project.title().as_str())
                .with_eyebrow("PROJECT")
                .with_field("ID", project.id().to_string())
                .with_field("Created", project.created_at())
                .with_field("Updated", project.updated_at())
        });
        let (empty_title, empty_description) = self.load_error.as_deref().map_or(
            ("No projects yet", "Create a project to group related work."),
            |error| ("Projects unavailable", error),
        );
        render_browser(
            frame,
            area,
            theme,
            &items,
            self.selected,
            details.as_ref(),
            empty_title,
            empty_description,
            &mut self.browser_hits,
        );
    }

    fn handle_mouse(&mut self, event: crossterm::event::MouseEvent, focus: FocusArea) -> bool {
        focus == FocusArea::Content
            && self.input_mode.is_none()
            && select_browser_row(&self.browser_hits, &mut self.selected, event)
    }

    fn dashboard_widgets(&mut self, layout: &DashboardLayout) -> Vec<DashboardWidget> {
        self.refresh();
        let id = DashboardWidgetId::new("tuilane.projects.recent")
            .expect("static dashboard widget identifier is valid");
        let descriptor = DashboardWidgetDescriptor::new(
            id.clone(),
            "Projects",
            "Your most recently updated projects",
            DashboardSize::Compact,
            vec![DashboardOption::new(
                "limit",
                "Visible projects",
                DashboardOptionKind::Number {
                    default: 4,
                    min: 1,
                    max: 8,
                },
            )],
        );
        let limit = layout
            .preference(&id)
            .and_then(|preference| preference.option("limit"))
            .and_then(|value| match value {
                DashboardOptionValue::Number(value) => Some(usize::from((*value).clamp(1, 8))),
                _ => None,
            })
            .unwrap_or(4);
        let content = self.load_error.as_ref().map_or_else(
            || DashboardWidgetContent::List {
                items: self
                    .projects
                    .iter()
                    .take(limit)
                    .map(|project| {
                        DashboardItem::new(
                            project.title().as_str(),
                            Some(format!("Updated {}", project.updated_at()).into_boxed_str()),
                        )
                    })
                    .collect(),
                empty_message: "No projects yet".into(),
            },
            |error| DashboardWidgetContent::Message {
                title: "Projects could not be loaded".into(),
                detail: error.as_str().into(),
                tone: DashboardTone::Critical,
            },
        );
        vec![DashboardWidget::new(descriptor, content)]
    }

    fn command_palette_actions(&self) -> Vec<ViewAction> {
        vec![ViewAction::new(
            "create",
            "Create project",
            "Create a project with a title",
        )]
    }

    fn activate_command_palette_action(&mut self, id: &str) -> bool {
        if id != "create" {
            return false;
        }
        self.input_mode = Some(InputMode::Title {
            id: None,
            value: String::new(),
        });
        self.status = None;
        true
    }

    fn dialog(&self) -> Option<ViewDialog> {
        match self.input_mode.as_ref()? {
            InputMode::Title { id, value } => Some(ViewDialog::Input {
                title: if id.is_some() {
                    "Rename project".to_owned()
                } else {
                    "New project".to_owned()
                },
                label: "Project title".to_owned(),
                value: value.clone(),
                error: self.status.clone(),
            }),
            InputMode::ConfirmDelete(id) => {
                let title = self
                    .projects
                    .iter()
                    .find(|project| project.id() == id)
                    .map_or("this project", |project| project.title().as_str());
                Some(ViewDialog::Confirm {
                    title: "Delete project".to_owned(),
                    message: format!("Delete '{title}'?"),
                })
            }
        }
    }

    fn take_notification(&mut self) -> Option<String> {
        if self.input_mode.is_some() {
            None
        } else {
            self.status.take()
        }
    }

    fn handle_key(&mut self, key: KeyEvent, focus: FocusArea) -> bool {
        if self.input_mode.is_none() && focus != FocusArea::Content {
            return false;
        }
        if let Some(mode) = self.input_mode.take() {
            match mode {
                InputMode::Title { id, mut value } => {
                    match key.code {
                        KeyCode::Esc => {}
                        KeyCode::Enter => self.finish_title_input(id, value),
                        KeyCode::Backspace => {
                            self.status = None;
                            value.pop();
                            self.input_mode = Some(InputMode::Title { id, value });
                        }
                        KeyCode::Char(character)
                            if !key.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                            ) =>
                        {
                            self.status = None;
                            value.push(character);
                            self.input_mode = Some(InputMode::Title { id, value });
                        }
                        _ => self.input_mode = Some(InputMode::Title { id, value }),
                    }
                    true
                }
                InputMode::ConfirmDelete(id) => {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('n') => {
                            self.status = None;
                        }
                        KeyCode::Char('y') => self
                            .run_command(ProjectCommand::Delete { id }, |_| {
                                "Deleted project.".to_owned()
                            }),
                        _ => self.input_mode = Some(InputMode::ConfirmDelete(id)),
                    }
                    true
                }
            }
        } else {
            match key.code {
                KeyCode::Char('c') => {
                    self.input_mode = Some(InputMode::Title {
                        id: None,
                        value: String::new(),
                    });
                    true
                }
                KeyCode::Char('e') => {
                    if let Some(project) = self.selected_project() {
                        self.input_mode = Some(InputMode::Title {
                            id: Some(project.id().clone()),
                            value: project.title().to_string(),
                        });
                    }
                    true
                }
                KeyCode::Delete => {
                    if let Some(project) = self.selected_project() {
                        self.input_mode = Some(InputMode::ConfirmDelete(project.id().clone()));
                    }
                    true
                }
                KeyCode::F(5) => {
                    self.status = None;
                    self.dirty.set(true);
                    true
                }
                KeyCode::Up => {
                    self.move_selection(-1);
                    true
                }
                KeyCode::Down => {
                    self.move_selection(1);
                    true
                }
                _ => false,
            }
        }
    }

    fn keybindings(&self, focus: FocusArea) -> Vec<Keybinding> {
        if self.input_mode.is_none() && focus != FocusArea::Content {
            return Vec::new();
        }

        match self.input_mode.as_ref() {
            Some(InputMode::Title { .. }) => vec![
                Keybinding::new("Save", "Enter"),
                Keybinding::new("Cancel", "Esc"),
            ],
            Some(InputMode::ConfirmDelete(_)) => vec![
                Keybinding::new("Confirm", "y"),
                Keybinding::new("Cancel", "n / Esc"),
            ],
            None => vec![
                Keybinding::new("Move", "↑/↓, j/k").with_priority(10),
                Keybinding::new("Create", "c").with_priority(90),
                Keybinding::new("Edit", "e").with_priority(80),
                Keybinding::new("Delete", "Delete").with_priority(60),
                Keybinding::new("Refresh", "F5").with_priority(5),
            ],
        }
    }

    fn render_inspector(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
        self.refresh();
        let Some(project) = self.selected_project() else {
            return false;
        };
        let text = format!(
            "{}\nID: {}\nCreated: {}\nUpdated: {}",
            project.title(),
            project.id(),
            project.created_at(),
            project.updated_at()
        );
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
        true
    }

    fn render_drawer(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
        let message = self
            .status
            .as_deref()
            .unwrap_or("Projects are saved in the local workspace database.");
        frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: true }), area);
        true
    }
}
