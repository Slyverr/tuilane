use std::{cell::Cell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use tuilane_core::{CommandDispatcher, EventSubscription};
use tuilane_plugin_api::ideas::{
    Idea, IdeaCommand, IdeaCommandResult, IdeaId, IdeaTitle, IdeasChangedV1,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, View, ViewAction};

enum InputMode {
    Title { id: Option<IdeaId>, value: String },
    ConfirmDelete(IdeaId),
}

/// Ideas view for capturing and organizing concise idea records.
pub struct IdeasView {
    descriptor: ViewDescriptor,
    commands: Rc<std::cell::RefCell<CommandDispatcher>>,
    ideas: Vec<Idea>,
    selected: Option<usize>,
    input_mode: Option<InputMode>,
    status: Option<String>,
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
}

impl IdeasView {
    /// Creates an Ideas view sharing the host's command and event paths.
    pub fn new(
        commands: Rc<std::cell::RefCell<CommandDispatcher>>,
        events: &tuilane_core::EventBus,
    ) -> Self {
        let dirty = Rc::new(Cell::new(true));
        let event_dirty = Rc::clone(&dirty);
        let event_subscription = events.subscribe(move |_: &IdeasChangedV1| {
            event_dirty.set(true);
        });

        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("ideas").expect("static view identifier is valid"),
                "Ideas",
                "Capture and organize ideas",
            ),
            commands,
            ideas: Vec::new(),
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
                    .execute(IdeaCommand::List)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(IdeaCommandResult::Ideas(ideas)) => {
                self.ideas = ideas;
                self.selected = match (self.selected, self.ideas.is_empty()) {
                    (_, true) => None,
                    (Some(index), false) => Some(index.min(self.ideas.len() - 1)),
                    (None, false) => Some(0),
                };
            }
            Ok(_) => self.status = Some("Idea list returned an unexpected response.".to_owned()),
            Err(error) => {
                self.dirty.set(true);
                self.status = Some(error);
            }
        }
    }

    fn selected_idea(&self) -> Option<&Idea> {
        self.selected.and_then(|index| self.ideas.get(index))
    }

    fn run_command(
        &mut self,
        command: IdeaCommand,
        success: impl FnOnce(&IdeaCommandResult) -> String,
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

    fn finish_title_input(&mut self, id: Option<IdeaId>, value: String) {
        let title = match IdeaTitle::new(&value) {
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
                    IdeaCommand::Rename { id, title },
                    |response| match response {
                        IdeaCommandResult::Idea(idea) => format!("Renamed to {}", idea.title()),
                        _ => "Renamed idea.".to_owned(),
                    },
                )
            }
            None => self.run_command(IdeaCommand::Create { title }, |response| match response {
                IdeaCommandResult::Idea(idea) => format!("Captured {}", idea.title()),
                _ => "Captured idea.".to_owned(),
            }),
        }
    }

    fn move_selection(&mut self, offset: isize) {
        if self.ideas.is_empty() {
            self.selected = None;
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        self.selected = Some((current + offset).rem_euclid(self.ideas.len() as isize) as usize);
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("c capture  e rename  Delete remove  ↑/↓ select")];
        if self.ideas.is_empty() {
            lines.push(Line::from("No ideas yet. Press c to capture one."));
        } else {
            lines.extend(self.ideas.iter().enumerate().map(|(index, idea)| {
                let marker = if Some(index) == self.selected {
                    "> "
                } else {
                    "  "
                };
                Line::from(format!("{marker}{}", idea.title()))
            }));
        }

        if let Some(idea) = self.selected_idea() {
            lines.push(Line::from(format!("\n# {}", idea.title())));
            lines.push(Line::from(format!("ID: {}", idea.id())));
            lines.push(Line::from(format!("Updated: {}", idea.updated_at())));
        }
        if let Some(InputMode::Title { value, .. }) = &self.input_mode {
            lines.push(Line::from(format!("Idea title: {value}_")));
        }
        if matches!(self.input_mode, Some(InputMode::ConfirmDelete(_))) {
            lines.push(Line::from(
                "Delete idea? Press y to confirm or Esc to cancel.",
            ));
        }
        if let Some(status) = &self.status {
            lines.push(Line::from(status.clone()));
        }
        lines
    }
}

impl View for IdeasView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn navigation_icon(&self) -> &'static str {
        "✦"
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.refresh();
        frame.render_widget(Paragraph::new(self.lines()).wrap(Wrap { trim: true }), area);
    }

    fn command_palette_actions(&self) -> Vec<ViewAction> {
        vec![ViewAction::new(
            "capture",
            "Capture idea",
            "Quickly record an idea",
        )]
    }

    fn activate_command_palette_action(&mut self, id: &str) -> bool {
        if id != "capture" {
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
                        self.run_command(IdeaCommand::Delete { id }, |response| match response {
                            IdeaCommandResult::Deleted(id) => format!("Deleted idea {id}"),
                            _ => "Deleted idea.".to_owned(),
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
                if let Some(idea) = self.selected_idea() {
                    self.input_mode = Some(InputMode::Title {
                        id: Some(idea.id().clone()),
                        value: idea.title().to_string(),
                    });
                }
                true
            }
            KeyCode::Delete => {
                if let Some(idea) = self.selected_idea() {
                    self.input_mode = Some(InputMode::ConfirmDelete(idea.id().clone()));
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
