use std::{cell::Cell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use tuilane_core::{CommandDispatcher, EventSubscription};
use tuilane_plugin_api::projects::{
    Project, ProjectCommand, ProjectCommandResult, ProjectId, ProjectTitle, ProjectsChangedV1,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, View, ViewAction};

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
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
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
            dirty,
            _event_subscription: event_subscription,
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
                self.projects = projects;
                self.selected = match (self.selected, self.projects.is_empty()) {
                    (_, true) => None,
                    (Some(index), false) => Some(index.min(self.projects.len() - 1)),
                    (None, false) => Some(0),
                };
            }
            Ok(_) => self.status = Some("Project list returned an unexpected response.".to_owned()),
            Err(error) => {
                self.dirty.set(true);
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

    fn project_lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("c create  e rename  Delete remove  ↑/↓ select")];
        if self.projects.is_empty() {
            lines.push(Line::from("No projects yet. Press c to create one."));
        } else {
            lines.extend(self.projects.iter().enumerate().map(|(index, project)| {
                let marker = if Some(index) == self.selected {
                    "> "
                } else {
                    "  "
                };
                Line::from(format!("{marker}{}", project.title()))
            }));
        }
        if let Some(InputMode::Title { value, .. }) = &self.input_mode {
            lines.push(Line::from(format!("Project title: {value}_")));
        }
        if matches!(self.input_mode, Some(InputMode::ConfirmDelete(_))) {
            lines.push(Line::from(
                "Delete project? Press y to confirm or Esc to cancel.",
            ));
        }
        if let Some(status) = &self.status {
            lines.push(Line::from(status.clone()));
        }
        lines
    }
}

impl View for ProjectsView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn navigation_icon(&self) -> &'static str {
        "▰"
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.refresh();
        frame.render_widget(
            Paragraph::new(self.project_lines()).wrap(Wrap { trim: true }),
            area,
        );
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

    fn handle_key(&mut self, key: KeyEvent, focus: FocusArea) -> bool {
        if self.input_mode.is_none() && focus != FocusArea::Content {
            return false;
        }
        if let Some(mode) = self.input_mode.take() {
            match mode {
                InputMode::Title { id, mut value } => {
                    match key.code {
                        KeyCode::Esc => self.status = Some("Edit cancelled.".to_owned()),
                        KeyCode::Enter => self.finish_title_input(id, value),
                        KeyCode::Backspace => {
                            value.pop();
                            self.input_mode = Some(InputMode::Title { id, value });
                        }
                        KeyCode::Char(character)
                            if !key.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                            ) =>
                        {
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
                            self.status = Some("Deletion cancelled.".to_owned());
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
