use std::{cell::Cell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
use tuilane_core::{CommandDispatcher, EventSubscription};
use tuilane_plugin_api::links::{
    ExternalReference, LinkCommand, LinkCommandResult, LinksChangedV1,
};
use tuilane_plugin_api::{
    ViewDescriptor, ViewId,
    items::{ItemRef, ItemSummary},
};
use tuilane_ui::{FocusArea, View};

#[derive(Clone, Copy)]
enum Direction {
    Outgoing,
    Backlinks,
    References,
}

enum InputMode {
    Item(String),
    Target(String),
    Reference(String),
}

/// Links view for following local relations and viewing external references.
pub struct LinksView {
    descriptor: ViewDescriptor,
    commands: Rc<std::cell::RefCell<CommandDispatcher>>,
    item: Option<ItemRef>,
    direction: Direction,
    items: Vec<ItemSummary>,
    references: Vec<ExternalReference>,
    selected: Option<usize>,
    input_mode: Option<InputMode>,
    status: Option<String>,
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
}

impl LinksView {
    /// Creates a Links view sharing the host's command and event paths.
    pub fn new(
        commands: Rc<std::cell::RefCell<CommandDispatcher>>,
        events: &tuilane_core::EventBus,
    ) -> Self {
        let dirty = Rc::new(Cell::new(true));
        let event_dirty = Rc::clone(&dirty);
        let event_subscription = events.subscribe(move |_: &LinksChangedV1| {
            event_dirty.set(true);
        });

        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("links").expect("static view identifier is valid"),
                "Links",
                "Navigate item relations and external references",
            ),
            commands,
            item: None,
            direction: Direction::Outgoing,
            items: Vec::new(),
            references: Vec::new(),
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
        let Some(item) = self.item.clone() else {
            self.items.clear();
            self.references.clear();
            return;
        };
        let command = match self.direction {
            Direction::Outgoing => LinkCommand::Outgoing { item },
            Direction::Backlinks => LinkCommand::Backlinks { item },
            Direction::References => LinkCommand::References { owner: item },
        };
        let result = self
            .commands
            .try_borrow_mut()
            .map_err(|_| "command dispatcher is busy".to_owned())
            .and_then(|mut commands| commands.execute(command).map_err(|error| error.to_string()));
        match result {
            Ok(LinkCommandResult::Items(items)) => {
                self.items = items;
                self.references.clear();
                self.clamp_selection();
            }
            Ok(LinkCommandResult::References(references)) => {
                self.references = references;
                self.items.clear();
                self.clamp_selection();
            }
            Ok(_) => self.status = Some("Link query returned an unexpected response.".to_owned()),
            Err(error) => {
                self.dirty.set(true);
                self.status = Some(error);
            }
        }
    }

    fn clamp_selection(&mut self) {
        let length = match self.direction {
            Direction::References => self.references.len(),
            Direction::Outgoing | Direction::Backlinks => self.items.len(),
        };
        self.selected = match (self.selected, length) {
            (_, 0) => None,
            (Some(index), length) => Some(index.min(length - 1)),
            (None, _) => Some(0),
        };
    }

    fn run_command(&mut self, command: LinkCommand, success: &'static str) {
        let result = self
            .commands
            .try_borrow_mut()
            .map_err(|_| "command dispatcher is busy".to_owned())
            .and_then(|mut commands| commands.execute(command).map_err(|error| error.to_string()));
        match result {
            Ok(_) => {
                self.status = Some(success.to_owned());
                self.dirty.set(true);
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn finish_input(&mut self, mode: InputMode, value: String) {
        match mode {
            InputMode::Item(_) => match value.parse::<ItemRef>() {
                Ok(item) => {
                    self.item = Some(item);
                    self.direction = Direction::Outgoing;
                    self.selected = None;
                    self.status = None;
                    self.dirty.set(true);
                }
                Err(error) => {
                    self.status = Some(error.to_string());
                    self.input_mode = Some(InputMode::Item(value));
                }
            },
            InputMode::Target(_) => {
                let Some(source) = self.item.clone() else {
                    self.status = Some("Choose a source item first.".to_owned());
                    return;
                };
                match value.parse::<ItemRef>() {
                    Ok(target) => {
                        self.run_command(LinkCommand::Add { source, target }, "Item link added.")
                    }
                    Err(error) => {
                        self.status = Some(error.to_string());
                        self.input_mode = Some(InputMode::Target(value));
                    }
                }
            }
            InputMode::Reference(_) => {
                let Some(owner) = self.item.clone() else {
                    self.status = Some("Choose an item first.".to_owned());
                    return;
                };
                let reference = if value.starts_with("http://") || value.starts_with("https://") {
                    ExternalReference::url(&value)
                } else {
                    ExternalReference::file(&value)
                };
                match reference {
                    Ok(reference) => self.run_command(
                        LinkCommand::AddReference { owner, reference },
                        "External reference added.",
                    ),
                    Err(error) => {
                        self.status = Some(error.to_string());
                        self.input_mode = Some(InputMode::Reference(value));
                    }
                }
            }
        }
    }

    fn move_selection(&mut self, offset: isize) {
        let length = match self.direction {
            Direction::References => self.references.len(),
            Direction::Outgoing | Direction::Backlinks => self.items.len(),
        };
        if length == 0 {
            self.selected = None;
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        self.selected = Some((current + offset).rem_euclid(length as isize) as usize);
    }

    fn remove_selected(&mut self) {
        let Some(item) = self.item.clone() else {
            return;
        };
        match self.direction {
            Direction::Outgoing => {
                if let Some(target) = self.selected.and_then(|index| self.items.get(index)) {
                    self.run_command(
                        LinkCommand::Remove {
                            source: item,
                            target: target.reference().clone(),
                        },
                        "Item link removed.",
                    );
                }
            }
            Direction::Backlinks => {
                if let Some(source) = self.selected.and_then(|index| self.items.get(index)) {
                    self.run_command(
                        LinkCommand::Remove {
                            source: source.reference().clone(),
                            target: item,
                        },
                        "Item link removed.",
                    );
                }
            }
            Direction::References => {
                if let Some(reference) = self.selected.and_then(|index| self.references.get(index))
                {
                    self.run_command(
                        LinkCommand::RemoveReference {
                            owner: item,
                            reference: reference.clone(),
                        },
                        "External reference removed.",
                    );
                }
            }
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from(
            "s select item  a add link  b backlinks  o outgoing  x add reference  Enter follow  Delete remove",
        )];
        lines.push(Line::from(format!(
            "Item: {}",
            self.item
                .as_ref()
                .map_or("(choose with s)".to_owned(), ToString::to_string)
        )));
        match self.direction {
            Direction::Outgoing | Direction::Backlinks => {
                let label = if matches!(self.direction, Direction::Outgoing) {
                    "Outgoing links"
                } else {
                    "Backlinks"
                };
                lines.push(Line::from(format!("\n{label}")));
                if self.items.is_empty() {
                    lines.push(Line::from("None."));
                } else {
                    lines.extend(self.items.iter().enumerate().map(|(index, summary)| {
                        let marker = if Some(index) == self.selected {
                            "> "
                        } else {
                            "  "
                        };
                        Line::from(format!(
                            "{marker}{}  {}",
                            summary.title(),
                            summary.reference()
                        ))
                    }));
                }
            }
            Direction::References => {
                lines.push(Line::from("\nFile and URL references"));
                if self.references.is_empty() {
                    lines.push(Line::from("None."));
                } else {
                    lines.extend(
                        self.references
                            .iter()
                            .enumerate()
                            .map(|(index, reference)| {
                                let marker = if Some(index) == self.selected {
                                    "> "
                                } else {
                                    "  "
                                };
                                Line::from(format!(
                                    "{marker}{}  {}",
                                    reference.kind().as_str(),
                                    reference.value()
                                ))
                            }),
                    );
                }
            }
        }
        if let Some(input) = &self.input_mode {
            let (label, value) = match input {
                InputMode::Item(value) => ("Item reference", value),
                InputMode::Target(value) => ("Link target", value),
                InputMode::Reference(value) => ("File path or URL", value),
            };
            lines.push(Line::from(format!("{label}: {value}_")));
        }
        if let Some(status) = &self.status {
            lines.push(Line::from(status.clone()));
        }
        lines
    }
}

impl View for LinksView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.refresh();
        frame.render_widget(Paragraph::new(self.lines()).wrap(Wrap { trim: true }), area);
    }

    fn handle_key(&mut self, key: KeyEvent, focus: FocusArea) -> bool {
        if self.input_mode.is_none() && focus != FocusArea::Content {
            return false;
        }
        if let Some(mode) = self.input_mode.take() {
            let mut value = match &mode {
                InputMode::Item(value) | InputMode::Target(value) | InputMode::Reference(value) => {
                    value.clone()
                }
            };
            match key.code {
                KeyCode::Esc => {
                    self.status = Some("Input cancelled.".to_owned());
                    return true;
                }
                KeyCode::Enter => {
                    self.finish_input(mode, value);
                    return true;
                }
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(character)
                    if !key.modifiers.intersects(
                        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                    ) =>
                {
                    value.push(character);
                }
                _ => {}
            }
            self.input_mode = Some(match mode {
                InputMode::Item(_) => InputMode::Item(value),
                InputMode::Target(_) => InputMode::Target(value),
                InputMode::Reference(_) => InputMode::Reference(value),
            });
            return true;
        }

        match key.code {
            KeyCode::Char('s') => {
                self.input_mode = Some(InputMode::Item(String::new()));
                true
            }
            KeyCode::Char('a') => {
                if self.item.is_some() {
                    self.input_mode = Some(InputMode::Target(String::new()));
                } else {
                    self.status = Some("Choose a source item first.".to_owned());
                }
                true
            }
            KeyCode::Char('x') => {
                if self.item.is_some() {
                    self.input_mode = Some(InputMode::Reference(String::new()));
                } else {
                    self.status = Some("Choose an item first.".to_owned());
                }
                true
            }
            KeyCode::Char('b') => {
                self.direction = Direction::Backlinks;
                self.selected = None;
                self.dirty.set(true);
                true
            }
            KeyCode::Char('o') => {
                self.direction = Direction::Outgoing;
                self.selected = None;
                self.dirty.set(true);
                true
            }
            KeyCode::Char('r') => {
                self.direction = Direction::References;
                self.selected = None;
                self.dirty.set(true);
                true
            }
            KeyCode::Enter if !matches!(self.direction, Direction::References) => {
                if let Some(summary) = self.selected.and_then(|index| self.items.get(index)) {
                    self.item = Some(summary.reference().clone());
                    self.direction = Direction::Outgoing;
                    self.selected = None;
                    self.dirty.set(true);
                }
                true
            }
            KeyCode::Delete => {
                self.remove_selected();
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                true
            }
            _ => false,
        }
    }
}
