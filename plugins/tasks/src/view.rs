use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use tuilane_core::CommandDispatcher;
use tuilane_plugin_api::tasks::{
    Task, TaskCommand, TaskCommandResult, TaskDate, TaskId, TaskTitle,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, View};

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
        let mut lines = vec![Line::from(
            "c create  e edit title  u due date  x complete/reopen  Delete remove  ↑/↓ select",
        )];
        if self.tasks.is_empty() {
            lines.push(Line::from("No tasks yet. Press c to create one."));
        }
        for (index, task) in self.tasks.iter().enumerate() {
            let selected = Some(index) == self.selected;
            let marker = if selected { ">" } else { " " };
            let state = match task.status() {
                tuilane_plugin_api::tasks::TaskStatus::Open => "[ ]",
                tuilane_plugin_api::tasks::TaskStatus::Completed => "[x]",
            };
            let due_date = task
                .due_date()
                .map_or_else(String::new, |date| format!("  due {}", date));
            lines.push(Line::from(format!(
                "{marker} {state} {}{due_date}",
                task.title()
            )));
        }

        match &self.input_mode {
            Some(InputMode::Title {
                action: TitleAction::Create,
                value,
            }) => {
                lines.push(Line::from(format!("New task: {value}_")));
                lines.push(Line::from("Enter saves; Esc cancels."));
            }
            Some(InputMode::Title {
                action: TitleAction::Rename(_),
                value,
            }) => {
                lines.push(Line::from(format!("New title: {value}_")));
                lines.push(Line::from("Enter saves; Esc cancels."));
            }
            Some(InputMode::DueDate { value, .. }) => {
                lines.push(Line::from(format!(
                    "Due date (YYYY-MM-DD, blank clears): {value}_"
                )));
                lines.push(Line::from("Enter saves; Esc cancels."));
            }
            Some(InputMode::ConfirmDelete(id)) => {
                let title = self
                    .tasks
                    .iter()
                    .find(|task| task.id() == id)
                    .map_or("this task", |task| task.title().as_str());
                lines.push(Line::from(format!(
                    "Delete '{title}'? y confirms; n cancels."
                )));
            }
            None => {}
        }
        if let Some(status) = &self.status {
            lines.push(Line::from(status.clone()));
        }
        lines
    }
}

impl View for TasksView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.refresh();
        frame.render_widget(
            Paragraph::new(self.task_lines()).wrap(Wrap { trim: true }),
            area,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, focus: FocusArea) -> bool {
        if self.input_mode.is_none() && focus != FocusArea::Content {
            return false;
        }
        if let Some(mode) = self.input_mode.take() {
            match mode {
                InputMode::Title { action, mut value } => {
                    match key.code {
                        KeyCode::Esc => self.status = Some("Edit cancelled.".to_owned()),
                        KeyCode::Enter => self.finish_text_input(action, value),
                        KeyCode::Backspace => {
                            value.pop();
                            self.input_mode = Some(InputMode::Title { action, value });
                        }
                        KeyCode::Char(character)
                            if !key.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                            ) =>
                        {
                            value.push(character);
                            self.input_mode = Some(InputMode::Title { action, value });
                        }
                        _ => self.input_mode = Some(InputMode::Title { action, value }),
                    }
                    true
                }
                InputMode::DueDate { id, mut value } => {
                    match key.code {
                        KeyCode::Esc => self.status = Some("Edit cancelled.".to_owned()),
                        KeyCode::Enter => self.finish_due_date_input(id, value),
                        KeyCode::Backspace => {
                            value.pop();
                            self.input_mode = Some(InputMode::DueDate { id, value });
                        }
                        KeyCode::Char(character)
                            if !key.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                            ) =>
                        {
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
                            self.status = Some("Deletion cancelled.".to_owned());
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
