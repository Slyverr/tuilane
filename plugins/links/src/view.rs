use std::{cell::Cell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect};
use tuilane_core::{CommandDispatcher, EventSubscription};
use tuilane_plugin_api::links::{
    ExternalReference, LinkCommand, LinkCommandResult, LinksChangedV1,
};
use tuilane_plugin_api::{
    DashboardItem, DashboardLayout, DashboardOption, DashboardOptionKind, DashboardOptionValue,
    DashboardSize, DashboardTone, DashboardWidget, DashboardWidgetContent,
    DashboardWidgetDescriptor, DashboardWidgetId, ViewDescriptor, ViewId,
    items::{ItemRef, ItemSummary},
};
use tuilane_ui::{
    BrowserDetails, BrowserHitMap, BrowserItem, BrowserTone, FocusArea, Keybinding, ThemePreset,
    View, ViewDialog, render_browser, select_browser_row,
};

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

fn short_detail(value: &str, limit: usize) -> Box<str> {
    if value.chars().count() <= limit {
        return value.into();
    }
    value
        .chars()
        .take(limit.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect::<String>()
        .into_boxed_str()
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
    load_error: Option<String>,
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
    browser_hits: BrowserHitMap,
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
        let Some(item) = self.item.clone() else {
            self.items.clear();
            self.references.clear();
            self.load_error = None;
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
                self.load_error = None;
                self.items = items;
                self.references.clear();
                self.clamp_selection();
            }
            Ok(LinkCommandResult::References(references)) => {
                self.load_error = None;
                self.references = references;
                self.items.clear();
                self.clamp_selection();
            }
            Ok(_) => {
                let error = "Link query returned an unexpected response.".to_owned();
                self.status = Some(error.clone());
                self.load_error = Some(error);
            }
            Err(error) => {
                self.status = Some(error.clone());
                self.load_error = Some(error);
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
}

impl View for LinksView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn icon(&self) -> &'static str {
        tuilane_ui::icons::LINKS
    }

    fn keybindings(&self, focus: FocusArea) -> Vec<Keybinding> {
        if self.input_mode.is_none() && focus != FocusArea::Content {
            return Vec::new();
        }

        if self.input_mode.is_some() {
            return vec![
                Keybinding::new("Submit", "Enter"),
                Keybinding::new("Cancel", "Esc"),
            ];
        }

        let mut bindings = vec![Keybinding::new("Move", "↑/↓, j/k").with_priority(10)];
        if !matches!(self.direction, Direction::References) {
            bindings.push(Keybinding::new("Open", "Enter").with_priority(70));
        }
        bindings.extend([
            Keybinding::new("Source", "s").with_priority(100),
            Keybinding::new("Link", "a").with_priority(90),
            Keybinding::new("Reference", "x").with_priority(80),
            Keybinding::new("Backlinks", "b").with_priority(70),
            Keybinding::new("Outgoing", "o").with_priority(60),
            Keybinding::new("References", "r").with_priority(50),
            Keybinding::new("Remove", "Delete").with_priority(40),
            Keybinding::new("Refresh", "F5").with_priority(5),
        ]);
        bindings
    }

    fn dialog(&self) -> Option<ViewDialog> {
        match self.input_mode.as_ref()? {
            InputMode::Item(value) => Some(ViewDialog::Input {
                title: "Open related item".to_owned(),
                label: "Item reference".to_owned(),
                value: value.clone(),
                error: self.status.clone(),
            }),
            InputMode::Target(value) => Some(ViewDialog::Input {
                title: "Add item link".to_owned(),
                label: "Target reference".to_owned(),
                value: value.clone(),
                error: self.status.clone(),
            }),
            InputMode::Reference(value) => Some(ViewDialog::Input {
                title: "Add external reference".to_owned(),
                label: "File path or URL".to_owned(),
                value: value.clone(),
                error: self.status.clone(),
            }),
        }
    }

    fn take_notification(&mut self) -> Option<String> {
        if self.input_mode.is_some() {
            None
        } else {
            self.status.take()
        }
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.render_with_theme(frame, area, ThemePreset::default());
    }

    fn render_with_theme(&mut self, frame: &mut Frame<'_>, area: Rect, theme: ThemePreset) {
        self.refresh();
        let items = match self.direction {
            Direction::Outgoing | Direction::Backlinks => self
                .items
                .iter()
                .map(|item| {
                    BrowserItem::new(item.title())
                        .with_icon(tuilane_ui::icons::LINKS)
                        .with_summary(item.reference().to_string())
                        .with_badge(item.reference().kind(), BrowserTone::Accent)
                })
                .collect::<Vec<_>>(),
            Direction::References => self
                .references
                .iter()
                .map(|reference| {
                    BrowserItem::new(reference.kind().as_str())
                        .with_icon(tuilane_ui::icons::LINKS)
                        .with_summary(reference.value())
                        .with_badge("External", BrowserTone::Default)
                })
                .collect::<Vec<_>>(),
        };
        let details = match self.direction {
            Direction::Outgoing | Direction::Backlinks => self
                .selected
                .and_then(|index| self.items.get(index))
                .map(|item| {
                    BrowserDetails::new(item.title())
                        .with_eyebrow(if matches!(self.direction, Direction::Outgoing) {
                            "OUTGOING LINK"
                        } else {
                            "BACKLINK"
                        })
                        .with_field("Item", item.reference().to_string())
                        .with_field(
                            "Source",
                            self.item
                                .as_ref()
                                .map_or_else(|| "None".to_owned(), ToString::to_string),
                        )
                }),
            Direction::References => self
                .selected
                .and_then(|index| self.references.get(index))
                .map(|reference| {
                    BrowserDetails::new(reference.kind().as_str())
                        .with_eyebrow("EXTERNAL REFERENCE")
                        .with_field("Value", reference.value())
                        .with_field(
                            "Owner",
                            self.item
                                .as_ref()
                                .map_or_else(|| "None".to_owned(), ToString::to_string),
                        )
                }),
        };
        let (empty_title, empty_description) = if let Some(error) = &self.load_error {
            ("Connections unavailable", error.as_str())
        } else if self.item.is_none() {
            (
                "Choose a source item",
                "Press s to select a task, project, note, or idea.",
            )
        } else if items.is_empty() {
            match self.direction {
                Direction::Outgoing => ("No outgoing links", "Add a link with a."),
                Direction::Backlinks => (
                    "No backlinks",
                    "Other items that link here will appear here.",
                ),
                Direction::References => (
                    "No external references",
                    "Add a file or URL reference with x.",
                ),
            }
        } else {
            ("No connections", "Choose another direction to browse.")
        };
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
        let id = DashboardWidgetId::new("tuilane.links.context")
            .expect("static dashboard widget identifier is valid");
        let descriptor = DashboardWidgetDescriptor::new(
            id.clone(),
            "Item connections",
            "Links, backlinks, or references for the current source item",
            DashboardSize::Compact,
            vec![DashboardOption::new(
                "limit",
                "Visible connections",
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
        let content = if let Some(error) = &self.load_error {
            DashboardWidgetContent::Message {
                title: "Connections unavailable".into(),
                detail: error.as_str().into(),
                tone: DashboardTone::Warning,
            }
        } else if self.item.is_none() {
            DashboardWidgetContent::Message {
                title: "Choose a source item".into(),
                detail: "Open Links and press s to start browsing connections.".into(),
                tone: DashboardTone::Muted,
            }
        } else {
            match self.direction {
                Direction::Outgoing | Direction::Backlinks => DashboardWidgetContent::List {
                    items: self
                        .items
                        .iter()
                        .take(limit)
                        .map(|item| {
                            DashboardItem::new(
                                item.title(),
                                Some(short_detail(&item.reference().to_string(), 80)),
                            )
                        })
                        .collect(),
                    empty_message: if matches!(self.direction, Direction::Outgoing) {
                        "No linked items".into()
                    } else {
                        "No backlinks".into()
                    },
                },
                Direction::References => DashboardWidgetContent::List {
                    items: self
                        .references
                        .iter()
                        .take(limit)
                        .map(|reference| {
                            DashboardItem::new(
                                reference.kind().as_str(),
                                Some(short_detail(reference.value(), 80)),
                            )
                        })
                        .collect(),
                    empty_message: "No external references".into(),
                },
            }
        };
        vec![DashboardWidget::new(descriptor, content)]
    }

    fn open_dashboard_widget(&mut self, id: &DashboardWidgetId) {
        if id.as_str() == "tuilane.links.context" {
            self.dirty.set(true);
        }
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
                    self.status = None;
                    return true;
                }
                KeyCode::Enter => {
                    self.finish_input(mode, value);
                    return true;
                }
                KeyCode::Backspace => {
                    self.status = None;
                    value.pop();
                }
                KeyCode::Char(character)
                    if !key.modifiers.intersects(
                        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                    ) =>
                {
                    self.status = None;
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
