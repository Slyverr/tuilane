use std::{cell::Cell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect};
use tuilane_core::{CommandDispatcher, EventSubscription};
use tuilane_plugin_api::ideas::{
    Idea, IdeaCommand, IdeaCommandResult, IdeaId, IdeaTitle, IdeasChangedV1,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{
    BrowserDetails, BrowserHitMap, BrowserItem, DashboardItem, DashboardLayout, DashboardOption,
    DashboardOptionKind, DashboardOptionValue, DashboardSize, DashboardTone, DashboardWidget,
    DashboardWidgetContent, DashboardWidgetDescriptor, DashboardWidgetId, FocusArea, Keybinding,
    ThemePreset, View, ViewAction, ViewDialog, render_browser, select_browser_row,
};

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
    load_error: Option<String>,
    dirty: Rc<Cell<bool>>,
    _event_subscription: EventSubscription,
    browser_hits: BrowserHitMap,
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
                    .execute(IdeaCommand::List)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(IdeaCommandResult::Ideas(ideas)) => {
                self.load_error = None;
                self.ideas = ideas;
                self.selected = match (self.selected, self.ideas.is_empty()) {
                    (_, true) => None,
                    (Some(index), false) => Some(index.min(self.ideas.len() - 1)),
                    (None, false) => Some(0),
                };
            }
            Ok(_) => {
                let error = "Idea list returned an unexpected response.".to_owned();
                self.status = Some(error.clone());
                self.load_error = Some(error);
            }
            Err(error) => {
                self.load_error = Some(error.clone());
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
}

impl View for IdeasView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn icon(&self) -> &'static str {
        tuilane_ui::icons::IDEAS
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.render_with_theme(frame, area, ThemePreset::default());
    }

    fn render_with_theme(&mut self, frame: &mut Frame<'_>, area: Rect, theme: ThemePreset) {
        self.refresh();
        let items = self
            .ideas
            .iter()
            .map(|idea| {
                let updated = idea.updated_at().get(..10).unwrap_or(idea.updated_at());
                BrowserItem::new(idea.title().as_str())
                    .with_icon(tuilane_ui::icons::IDEAS)
                    .with_summary(format!("Updated {updated}"))
            })
            .collect::<Vec<_>>();
        let details = self.selected_idea().map(|idea| {
            BrowserDetails::new(idea.title().as_str())
                .with_eyebrow("IDEA")
                .with_field("ID", idea.id().to_string())
                .with_field("Created", idea.created_at())
                .with_field("Updated", idea.updated_at())
        });
        let (empty_title, empty_description) = self.load_error.as_deref().map_or(
            ("No ideas yet", "Capture an idea when it comes to mind."),
            |error| ("Ideas unavailable", error),
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
        let id = DashboardWidgetId::new("tuilane.ideas.recent")
            .expect("static dashboard widget identifier is valid");
        let descriptor = DashboardWidgetDescriptor::new(
            id.clone(),
            "Recent ideas",
            "Captured ideas, newest first",
            DashboardSize::Compact,
            vec![DashboardOption::new(
                "limit",
                "Visible ideas",
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
                    .ideas
                    .iter()
                    .take(limit)
                    .map(|idea| {
                        let updated = idea.updated_at().get(..10).unwrap_or(idea.updated_at());
                        DashboardItem::new(
                            idea.title().as_str(),
                            Some(format!("Updated {updated}").into_boxed_str()),
                        )
                    })
                    .collect(),
                empty_message: "No ideas yet".into(),
            },
            |error| DashboardWidgetContent::Message {
                title: "Ideas could not be loaded".into(),
                detail: error.as_str().into(),
                tone: DashboardTone::Critical,
            },
        );
        vec![DashboardWidget::new(descriptor, content)]
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

    fn dialog(&self) -> Option<ViewDialog> {
        match self.input_mode.as_ref()? {
            InputMode::Title { id, value } => Some(ViewDialog::Input {
                title: if id.is_some() {
                    "Rename idea".to_owned()
                } else {
                    "Capture idea".to_owned()
                },
                label: "Idea title".to_owned(),
                value: value.clone(),
                error: self.status.clone(),
            }),
            InputMode::ConfirmDelete(id) => {
                let title = self
                    .ideas
                    .iter()
                    .find(|idea| idea.id() == id)
                    .map_or("this idea", |idea| idea.title().as_str());
                Some(ViewDialog::Confirm {
                    title: "Delete idea".to_owned(),
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
                InputMode::Title { id, mut value } => match key.code {
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
                },
                InputMode::ConfirmDelete(id) => match key.code {
                    KeyCode::Esc | KeyCode::Char('n') => {
                        self.status = None;
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
                Keybinding::new("Capture", "c").with_priority(90),
                Keybinding::new("Rename", "e").with_priority(80),
                Keybinding::new("Delete", "Delete").with_priority(60),
                Keybinding::new("Refresh", "F5").with_priority(5),
            ],
        }
    }
}
