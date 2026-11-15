use std::{cell::Cell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
    widgets::{Paragraph, Wrap},
};
use tuilane_core::{CommandDispatcher, EventSubscription};
use tuilane_plugin_api::notes::{
    Markdown, Note, NoteCommand, NoteCommandResult, NoteId, NoteTitle, NotesChangedV1,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, Keybinding, View, ViewAction, render_empty_state};

enum InputMode {
    Title { id: Option<NoteId>, value: String },
    ConfirmDelete(NoteId),
}

/// Notes view for browsing Markdown and managing note titles.
pub struct NotesView {
    descriptor: ViewDescriptor,
    commands: Rc<std::cell::RefCell<CommandDispatcher>>,
    notes: Vec<Note>,
    selected: Option<usize>,
    input_mode: Option<InputMode>,
    status: Option<String>,
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
}

impl NotesView {
    /// Creates a Notes view sharing the host's command and event paths.
    pub fn new(
        commands: Rc<std::cell::RefCell<CommandDispatcher>>,
        events: &tuilane_core::EventBus,
    ) -> Self {
        let dirty = Rc::new(Cell::new(true));
        let event_dirty = Rc::clone(&dirty);
        let event_subscription = events.subscribe(move |_: &NotesChangedV1| {
            event_dirty.set(true);
        });

        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("notes").expect("static view identifier is valid"),
                "Notes",
                "Browse and capture Markdown notes",
            ),
            commands,
            notes: Vec::new(),
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
                    .execute(NoteCommand::List)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(NoteCommandResult::Notes(notes)) => {
                self.notes = notes;
                self.selected = match (self.selected, self.notes.is_empty()) {
                    (_, true) => None,
                    (Some(index), false) => Some(index.min(self.notes.len() - 1)),
                    (None, false) => Some(0),
                };
            }
            Ok(_) => self.status = Some("Note list returned an unexpected response.".to_owned()),
            Err(error) => {
                self.dirty.set(true);
                self.status = Some(error);
            }
        }
    }

    fn selected_note(&self) -> Option<&Note> {
        self.selected.and_then(|index| self.notes.get(index))
    }

    fn run_command(
        &mut self,
        command: NoteCommand,
        success: impl FnOnce(&NoteCommandResult) -> String,
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

    fn finish_title_input(&mut self, id: Option<NoteId>, value: String) {
        let title = match NoteTitle::new(&value) {
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
                    NoteCommand::Rename { id, title },
                    |response| match response {
                        NoteCommandResult::Note(note) => format!("Renamed to {}", note.title()),
                        _ => "Renamed note.".to_owned(),
                    },
                )
            }
            None => self.run_command(
                NoteCommand::Create {
                    title,
                    body: Markdown::new("").expect("empty Markdown is valid"),
                },
                |response| match response {
                    NoteCommandResult::Note(note) => format!("Created {}", note.title()),
                    _ => "Created note.".to_owned(),
                },
            ),
        }
    }

    fn move_selection(&mut self, offset: isize) {
        if self.notes.is_empty() {
            self.selected = None;
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        self.selected = Some((current + offset).rem_euclid(self.notes.len() as isize) as usize);
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = self
            .notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                let marker = if Some(index) == self.selected {
                    "> "
                } else {
                    "  "
                };
                Line::from(format!("{marker}{}", note.title()))
            })
            .collect::<Vec<_>>();

        if let Some(note) = self.selected_note() {
            lines.push(Line::from(format!("\n# {}", note.title())));
            lines.extend(
                note.body()
                    .as_str()
                    .lines()
                    .map(|line| Line::from(line.to_owned())),
            );
            if note.body().as_str().is_empty() {
                lines.push(Line::from("(empty Markdown)"));
            }
        }

        if let Some(InputMode::Title { value, .. }) = &self.input_mode {
            lines.push(Line::from(format!("Note title: {value}_")));
        }
        if matches!(self.input_mode, Some(InputMode::ConfirmDelete(_))) {
            lines.push(Line::from("Delete this note?"));
        }
        if let Some(status) = &self.status {
            lines.push(Line::from(status.clone()));
        }
        lines
    }
}

impl View for NotesView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn icon(&self) -> &'static str {
        "≡"
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.refresh();
        if self.notes.is_empty() && self.input_mode.is_none() && self.status.is_none() {
            render_empty_state(
                frame,
                area,
                "No notes yet",
                "Create a note to keep useful information close.",
            );
        } else {
            frame.render_widget(
                Paragraph::new(Text::from(self.lines())).wrap(Wrap { trim: true }),
                area,
            );
        }
    }

    fn command_palette_actions(&self) -> Vec<ViewAction> {
        vec![ViewAction::new(
            "create",
            "Create note",
            "Create a Markdown note",
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
                InputMode::Title { id, mut value } => match key.code {
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
                },
                InputMode::ConfirmDelete(id) => match key.code {
                    KeyCode::Esc | KeyCode::Char('n') => {
                        self.status = Some("Delete cancelled.".to_owned());
                    }
                    KeyCode::Char('y') => {
                        self.run_command(NoteCommand::Delete { id }, |response| match response {
                            NoteCommandResult::Deleted(id) => format!("Deleted note {id}"),
                            _ => "Deleted note.".to_owned(),
                        })
                    }
                    _ => self.input_mode = Some(InputMode::ConfirmDelete(id)),
                },
            }
            return true;
        }

        match key.code {
            KeyCode::Char('c') => {
                self.input_mode = Some(InputMode::Title {
                    id: None,
                    value: String::new(),
                });
                true
            }
            KeyCode::Char('e') => {
                if let Some(note) = self.selected_note() {
                    self.input_mode = Some(InputMode::Title {
                        id: Some(note.id().clone()),
                        value: note.title().to_string(),
                    });
                }
                true
            }
            KeyCode::Char('m') => {
                if let Some(note) = self.selected_note() {
                    self.status = Some(format!(
                        "Edit Markdown with: tuilane note edit {}",
                        note.id()
                    ));
                }
                true
            }
            KeyCode::Delete => {
                if let Some(note) = self.selected_note() {
                    self.input_mode = Some(InputMode::ConfirmDelete(note.id().clone()));
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
                Keybinding::new("Rename", "e").with_priority(80),
                Keybinding::new("Edit", "m").with_priority(70),
                Keybinding::new("Delete", "Delete").with_priority(60),
            ],
        }
    }
}
