use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use tuilane_core::CommandDispatcher;
use tuilane_plugin_api::tasks::{
    Task, TaskCommand, TaskCommandResult, TaskDate, TaskId, TaskTitle,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, Keybinding, View, ViewAction, ViewDialog, render_empty_state};

enum InputMode {
    Title { action: TitleAction, value: String },
    DueDate { id: TaskId, value: String },
    ConfirmDelete(TaskId),
}

enum TitleAction {
    Create,
    Rename(TaskId),
}

/// Tasks view that sends every mutation through the host command dispatcher.
pub struct TasksView {
    descriptor: ViewDescriptor,
    commands: Rc<RefCell<CommandDispatcher>>,
    tasks: Vec<Task>,
    selected: Option<usize>,
    input_mode: Option<InputMode>,
    status: Option<String>,
    dirty: bool,
}

impl TasksView {
    /// Creates a Tasks view sharing the host's application command path.
    pub fn new(commands: Rc<RefCell<CommandDispatcher>>) -> Self {
        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("tasks").expect("static view identifier is valid"),
                "Tasks",
                "Capture and manage local tasks",
            ),
            commands,
            tasks: Vec::new(),
            selected: None,
            input_mode: None,
            status: None,
            dirty: true,
        }
    }

    fn refresh(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let result = self
            .commands
            .try_borrow_mut()
            .map_err(|_| "command dispatcher is busy".to_owned())
            .and_then(|mut commands| {
                commands
                    .execute(TaskCommand::List {
                        include_completed: true,
                    })
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(TaskCommandResult::Tasks(tasks)) => {
                self.tasks = tasks;
                self.selected = match (self.selected, self.tasks.is_empty()) {
                    (_, true) => None,
                    (Some(index), false) => Some(index.min(self.tasks.len() - 1)),
                    (None, false) => Some(0),
                };
            }
            Ok(_) => self.status = Some("Task list returned an unexpected response.".to_owned()),
            Err(error) => self.status = Some(error),
        }
    }

    fn selected_task(&self) -> Option<&Task> {
        self.selected.and_then(|index| self.tasks.get(index))
    }

    fn run_command(
        &mut self,
        command: TaskCommand,
        success: impl FnOnce(&TaskCommandResult) -> String,
    ) {
        let result = self
            .commands
            .try_borrow_mut()
            .map_err(|_| "command dispatcher is busy".to_owned())
            .and_then(|mut commands| commands.execute(command).map_err(|error| error.to_string()));
        match result {
            Ok(response) => {
                self.status = Some(success(&response));
                self.dirty = true;
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn finish_text_input(&mut self, action: TitleAction, value: String) {
        let title = match TaskTitle::new(&value) {
            Ok(title) => title,
            Err(error) => {
                self.status = Some(error.to_string());
                self.input_mode = Some(InputMode::Title { action, value });
                return;
            }
        };

        match action {
            TitleAction::Create => self.run_command(
                TaskCommand::Create {
                    title,
                    due_date: None,
                },
                |response| match response {
                    TaskCommandResult::Task(task) => format!("Created {}", task.title()),
                    _ => "Created task.".to_owned(),
                },
            ),
            TitleAction::Rename(id) => self.run_command(
                TaskCommand::Rename { id, title },
                |response| match response {
                    TaskCommandResult::Task(task) => format!("Renamed to {}", task.title()),
                    _ => "Updated task.".to_owned(),
                },
            ),
        }
    }

    fn finish_due_date_input(&mut self, id: TaskId, value: String) {
        let due_date = if value.trim().is_empty() {
            None
        } else {
            match TaskDate::parse(value.trim()) {
                Ok(date) => Some(date),
                Err(error) => {
                    self.status = Some(error.to_string());
                    self.input_mode = Some(InputMode::DueDate { id, value });
                    return;
                }
            }
        };

        self.run_command(
            TaskCommand::SetDueDate { id, due_date },
            |response| match response {
                TaskCommandResult::Task(task) => task.due_date().map_or_else(
                    || format!("Cleared due date for {}", task.title()),
                    |date| format!("Due date set to {date}"),
                ),
                _ => "Updated due date.".to_owned(),
            },
        );
    }

    fn move_selection(&mut self, offset: isize) {
        if self.tasks.is_empty() {
            self.selected = None;
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        self.selected = Some((current + offset).rem_euclid(self.tasks.len() as isize) as usize);
    }

    fn task_lines(&self) -> Vec<Line<'static>> {
        let key_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let mut lines = Vec::new();
        for (index, task) in self.tasks.iter().enumerate() {
            let selected = Some(index) == self.selected;
            let marker = if selected { "›" } else { " " };
            let (state, state_style) = match task.status() {
                tuilane_plugin_api::tasks::TaskStatus::Open => {
                    ("○", Style::default().fg(Color::Cyan))
                }
                tuilane_plugin_api::tasks::TaskStatus::Completed => {
                    ("✓", Style::default().fg(Color::Green))
                }
            };
            let due_date = task
                .due_date()
                .map_or_else(String::new, |date| format!("  ◷ {date}"));
            lines.push(Line::from(vec![
                Span::styled(marker, key_style),
                Span::raw(" "),
                Span::styled(state, state_style),
                Span::raw("  "),
                Span::styled(
                    task.title().as_str().to_owned(),
                    if selected {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(due_date, Style::default().fg(Color::Yellow)),
            ]));
        }

        lines
    }
}

impl View for TasksView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn icon(&self) -> &'static str {
        tuilane_ui::icons::TASKS
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.refresh();
        if self.tasks.is_empty() && self.input_mode.is_none() {
            render_empty_state(
                frame,
                area,
                "No tasks yet",
                "Create a task to start organizing your work.",
            );
        } else {
            frame.render_widget(
                Paragraph::new(self.task_lines()).wrap(Wrap { trim: true }),
                area,
            );
        }
    }

    fn command_palette_actions(&self) -> Vec<ViewAction> {
        vec![ViewAction::new(
            "create",
            "Create task",
            "Capture a task with a title",
        )]
    }

    fn activate_command_palette_action(&mut self, id: &str) -> bool {
        if id != "create" {
            return false;
        }
        self.input_mode = Some(InputMode::Title {
            action: TitleAction::Create,
            value: String::new(),
        });
        self.status = None;
        true
    }

    fn dialog(&self) -> Option<ViewDialog> {
        match self.input_mode.as_ref()? {
            InputMode::Title {
                action: TitleAction::Create,
                value,
            } => Some(ViewDialog::Input {
                title: "New task".to_owned(),
                label: "Task title".to_owned(),
                value: value.clone(),
                error: self.status.clone(),
            }),
            InputMode::Title {
                action: TitleAction::Rename(_),
                value,
            } => Some(ViewDialog::Input {
                title: "Rename task".to_owned(),
                label: "Task title".to_owned(),
                value: value.clone(),
                error: self.status.clone(),
            }),
            InputMode::DueDate { value, .. } => Some(ViewDialog::Input {
                title: "Set due date".to_owned(),
                label: "Date (YYYY-MM-DD; blank clears)".to_owned(),
                value: value.clone(),
                error: self.status.clone(),
            }),
            InputMode::ConfirmDelete(id) => {
                let title = self
                    .tasks
                    .iter()
                    .find(|task| task.id() == id)
                    .map_or("this task", |task| task.title().as_str());
                Some(ViewDialog::Confirm {
                    title: "Delete task".to_owned(),
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
                InputMode::Title { action, mut value } => {
                    match key.code {
                        KeyCode::Esc => {}
                        KeyCode::Enter => self.finish_text_input(action, value),
                        KeyCode::Backspace => {
                            self.status = None;
                            value.pop();
                            self.input_mode = Some(InputMode::Title { action, value });
                        }
                        KeyCode::Char(character)
                            if !key.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                            ) =>
                        {
                            self.status = None;
                            value.push(character);
                            self.input_mode = Some(InputMode::Title { action, value });
                        }
                        _ => self.input_mode = Some(InputMode::Title { action, value }),
                    }
                    true
                }
                InputMode::DueDate { id, mut value } => {
                    match key.code {
                        KeyCode::Esc => {}
                        KeyCode::Enter => self.finish_due_date_input(id, value),
                        KeyCode::Backspace => {
                            self.status = None;
                            value.pop();
                            self.input_mode = Some(InputMode::DueDate { id, value });
                        }
                        KeyCode::Char(character)
                            if !key.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                            ) =>
                        {
                            self.status = None;
                            value.push(character);
                            self.input_mode = Some(InputMode::DueDate { id, value });
                        }
                        _ => self.input_mode = Some(InputMode::DueDate { id, value }),
                    }
                    true
                }
                InputMode::ConfirmDelete(id) => {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('n') => {
                            self.status = None;
                        }
                        KeyCode::Char('y') => self.run_command(TaskCommand::Delete { id }, |_| {
                            "Deleted task.".to_owned()
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
                        action: TitleAction::Create,
                        value: String::new(),
                    });
                    true
                }
                KeyCode::Char('e') => {
                    if let Some(task) = self.selected_task() {
                        self.input_mode = Some(InputMode::Title {
                            action: TitleAction::Rename(task.id().clone()),
                            value: task.title().to_string(),
                        });
                    }
                    true
                }
                KeyCode::Char('u') => {
                    if let Some(task) = self.selected_task() {
                        self.input_mode = Some(InputMode::DueDate {
                            id: task.id().clone(),
                            value: task
                                .due_date()
                                .map_or_else(String::new, ToString::to_string),
                        });
                    }
                    true
                }
                KeyCode::Char('x') => {
                    if let Some(task) = self.selected_task() {
                        let command = match task.status() {
                            tuilane_plugin_api::tasks::TaskStatus::Open => TaskCommand::Complete {
                                id: task.id().clone(),
                            },
                            tuilane_plugin_api::tasks::TaskStatus::Completed => {
                                TaskCommand::Reopen {
                                    id: task.id().clone(),
                                }
                            }
                        };
                        self.run_command(command, |response| match response {
                            TaskCommandResult::Task(task) => {
                                format!("Task is {}", task.status())
                            }
                            _ => "Updated task.".to_owned(),
                        });
                    }
                    true
                }
                KeyCode::Delete => {
                    if let Some(task) = self.selected_task() {
                        self.input_mode = Some(InputMode::ConfirmDelete(task.id().clone()));
                    }
                    true
                }
                KeyCode::F(5) => {
                    self.status = None;
                    self.dirty = true;
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
            Some(InputMode::DueDate { .. }) => vec![
                Keybinding::new("Save date", "Enter"),
                Keybinding::new("Cancel", "Esc"),
            ],
            Some(InputMode::ConfirmDelete(_)) => vec![
                Keybinding::new("Confirm", "y"),
                Keybinding::new("Cancel", "n / Esc"),
            ],
            None => vec![
                Keybinding::new("Move", "↑/↓, j/k").with_priority(10),
                Keybinding::new("Create", "c").with_priority(90),
                Keybinding::new("Rename", "e").with_priority(80),
                Keybinding::new("Due", "u").with_priority(70),
                Keybinding::new("Complete", "x").with_priority(60),
                Keybinding::new("Delete", "Delete").with_priority(50),
                Keybinding::new("Refresh", "F5").with_priority(5),
            ],
        }
    }

    fn render_inspector(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
        self.refresh();
        let Some(task) = self.selected_task() else {
            return false;
        };
        let due_date = task.due_date().map_or("None", TaskDate::as_str);
        let text = format!(
            "{}\nStatus: {}\nDue: {}\nCreated: {}\nUpdated: {}",
            task.title(),
            task.status(),
            due_date,
            task.created_at(),
            task.updated_at()
        );
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
        true
    }

    fn render_drawer(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
        let text = self
            .status
            .as_deref()
            .unwrap_or("Tasks changes are saved to the local database.");
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
        true
    }
}
