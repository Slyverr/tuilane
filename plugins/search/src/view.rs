use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::Paragraph,
};
use tuilane_core::CommandDispatcher;
use tuilane_plugin_api::search::{
    ListSavedSearchesCommand, SaveSavedSearchCommand, SavedSearch, SavedSearchName, SearchCommand,
    SearchCommandResult, SearchQuery, SearchResult,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, View, ViewAction};

/// Search workspace items through the shared host command path.
pub struct SearchView {
    descriptor: ViewDescriptor,
    commands: Rc<RefCell<CommandDispatcher>>,
    query: String,
    kind: Option<String>,
    editing: bool,
    saving_name: bool,
    save_name: String,
    saved_searches_open: bool,
    saved_searches: Vec<SavedSearch>,
    saved_selected: usize,
    results: Vec<SearchResult>,
    selected: usize,
    status: Option<String>,
}

impl SearchView {
    /// Creates a search view sharing the host's command dispatcher.
    pub fn new(commands: Rc<RefCell<CommandDispatcher>>) -> Self {
        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("search").expect("static view identifier is valid"),
                "Search",
                "Find tasks, projects, notes, and ideas",
            ),
            commands,
            query: String::new(),
            kind: None,
            editing: false,
            saving_name: false,
            save_name: String::new(),
            saved_searches_open: false,
            saved_searches: Vec::new(),
            saved_selected: 0,
            results: Vec::new(),
            selected: 0,
            status: None,
        }
    }

    fn execute_search(&mut self) {
        let query = match SearchQuery::new(&self.query) {
            Ok(query) => query,
            Err(error) => {
                self.status = Some(error.to_string());
                return;
            }
        };
        let command = match self.kind.as_deref() {
            Some(kind) => SearchCommand::new(query).with_kind(kind),
            None => Ok(SearchCommand::new(query)),
        };
        let command = match command {
            Ok(command) => command,
            Err(error) => {
                self.status = Some(error.to_string());
                return;
            }
        };
        match self.commands.try_borrow_mut() {
            Ok(mut commands) => match commands.execute(command) {
                Ok(SearchCommandResult::Results(results)) => {
                    self.selected = 0;
                    self.results = results;
                    self.status = Some(format!("{} matches", self.results.len()));
                }
                Err(error) => self.status = Some(error.to_string()),
            },
            Err(_) => self.status = Some("The command dispatcher is busy.".to_owned()),
        }
    }

    fn open_saved_searches(&mut self) {
        match self.commands.try_borrow_mut() {
            Ok(mut commands) => match commands.execute(ListSavedSearchesCommand) {
                Ok(saved_searches) => {
                    self.saved_searches = saved_searches;
                    self.saved_selected = 0;
                    self.saved_searches_open = true;
                    self.status = None;
                }
                Err(error) => self.status = Some(error.to_string()),
            },
            Err(_) => self.status = Some("The command dispatcher is busy.".to_owned()),
        }
    }

    fn activate_saved_search(&mut self) {
        let Some(saved) = self.saved_searches.get(self.saved_selected) else {
            return;
        };
        self.query = saved.query().as_str().to_owned();
        self.kind = saved.kind().map(str::to_owned);
        self.saved_searches_open = false;
        self.execute_search();
    }

    fn cycle_kind_filter(&mut self) {
        let kinds = [
            None,
            Some("task"),
            Some("project"),
            Some("note"),
            Some("idea"),
        ];
        let current = self.kind.as_deref();
        let next = kinds
            .iter()
            .position(|kind| *kind == current)
            .map_or(0, |index| (index + 1) % kinds.len());
        self.kind = kinds[next].map(str::to_owned);
        if !self.query.is_empty() {
            self.execute_search();
        }
    }

    fn save_current_search(&mut self) {
        let result = SavedSearchName::new(&self.save_name)
            .map_err(|error| error.to_string())
            .and_then(|name| {
                let query = SearchQuery::new(&self.query).map_err(|error| error.to_string())?;
                SavedSearch::new(name, query, self.kind.as_deref())
                    .map_err(|error| error.to_string())
            })
            .and_then(|saved| {
                self.commands
                    .try_borrow_mut()
                    .map_err(|_| "The command dispatcher is busy.".to_owned())?
                    .execute(SaveSavedSearchCommand::new(saved))
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(saved) => {
                self.status = Some(format!("Saved search: {}", saved.name().as_str()));
                self.saving_name = false;
                self.save_name.clear();
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn move_selection(&mut self, forward: bool) {
        if self.results.is_empty() {
            self.selected = 0;
        } else if forward {
            self.selected = (self.selected + 1) % self.results.len();
        } else {
            self.selected = (self.selected + self.results.len() - 1) % self.results.len();
        }
    }

    fn result_lines(&self) -> Vec<Line<'static>> {
        if self.results.is_empty() {
            return vec![Line::from("No matches yet. Press / to search.")];
        }
        self.results
            .iter()
            .enumerate()
            .map(|(index, result)| {
                let marker = if index == self.selected { ">" } else { " " };
                Line::from(format!(
                    "{marker} [{}] {}  {}",
                    result.item().reference().kind(),
                    result.item().title(),
                    result.item().reference()
                ))
            })
            .collect()
    }
}

impl View for SearchView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn navigation_icon(&self) -> &'static str {
        "⌕"
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = if self.saving_name {
            vec![
                Line::from(format!("Save search as: {}_", self.save_name)),
                Line::from("Enter saves; Esc cancels."),
            ]
        } else if self.saved_searches_open {
            let mut lines = vec![Line::from("Saved searches — Enter runs; Esc closes")];
            if self.saved_searches.is_empty() {
                lines.push(Line::from(
                    "No saved searches. Save one from the command palette.",
                ));
            } else {
                lines.extend(
                    self.saved_searches
                        .iter()
                        .enumerate()
                        .map(|(index, saved)| {
                            let marker = if index == self.saved_selected {
                                "> "
                            } else {
                                "  "
                            };
                            Line::from(format!(
                                "{marker}{}  [{}] {}",
                                saved.name().as_str(),
                                saved.kind().unwrap_or("all"),
                                saved.query().as_str()
                            ))
                        }),
                );
            }
            lines
        } else {
            let mut lines = vec![Line::from(if self.editing {
                format!("/{}", self.query)
            } else if self.query.is_empty() {
                "Press / to search workspace items".to_owned()
            } else {
                format!("Query: {}", self.query)
            })];
            if !self.editing {
                lines.push(Line::from(format!(
                    "Filter: {} — press f to change",
                    self.kind.as_deref().unwrap_or("all items")
                )));
            }
            lines.extend(self.result_lines());
            lines
        };
        if let Some(status) = &self.status {
            lines.push(Line::from(status.clone()));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .style(Style::default())
                .wrap(ratatui::widgets::Wrap { trim: true }),
            area,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, focus: FocusArea) -> bool {
        if focus != FocusArea::Content {
            return false;
        }
        if self.saving_name {
            match key.code {
                KeyCode::Esc => {
                    self.saving_name = false;
                    self.save_name.clear();
                    self.status = Some("Saving search cancelled.".to_owned());
                }
                KeyCode::Enter => self.save_current_search(),
                KeyCode::Backspace => {
                    self.save_name.pop();
                }
                KeyCode::Char(character)
                    if !key.modifiers.intersects(
                        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                    ) && self.save_name.chars().count() < 64 =>
                {
                    self.save_name.push(character);
                }
                _ => {}
            }
            return true;
        }
        if self.saved_searches_open {
            match key.code {
                KeyCode::Esc => self.saved_searches_open = false,
                KeyCode::Up => {
                    if !self.saved_searches.is_empty() {
                        self.saved_selected = (self.saved_selected + self.saved_searches.len() - 1)
                            % self.saved_searches.len();
                    }
                }
                KeyCode::Down => {
                    if !self.saved_searches.is_empty() {
                        self.saved_selected = (self.saved_selected + 1) % self.saved_searches.len();
                    }
                }
                KeyCode::Enter => self.activate_saved_search(),
                _ => {}
            }
            return true;
        }
        if self.editing {
            match key.code {
                KeyCode::Esc => self.editing = false,
                KeyCode::Enter => {
                    self.editing = false;
                    self.execute_search();
                }
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char(character)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && self.query.chars().count() < 256 =>
                {
                    self.query.push(character);
                }
                _ => {}
            }
            return true;
        }
        match key.code {
            KeyCode::Char('/') => {
                self.query.clear();
                self.editing = true;
                self.status = None;
                true
            }
            KeyCode::Char('f') => {
                self.cycle_kind_filter();
                true
            }
            KeyCode::Up => {
                self.move_selection(false);
                !self.results.is_empty()
            }
            KeyCode::Down => {
                self.move_selection(true);
                !self.results.is_empty()
            }
            _ => false,
        }
    }

    fn command_palette_actions(&self) -> Vec<ViewAction> {
        if self.editing || self.saving_name || self.saved_searches_open {
            return Vec::new();
        }
        let mut actions = vec![ViewAction::new(
            "saved-searches",
            "Browse saved searches",
            "Choose a saved search view to run",
        )];
        if SearchQuery::new(&self.query).is_ok() {
            actions.push(ViewAction::new(
                "save-search",
                "Save current search",
                "Save the current query and item-kind filter",
            ));
        }
        actions
    }

    fn activate_command_palette_action(&mut self, id: &str) -> bool {
        if self.editing || self.saving_name || self.saved_searches_open {
            return false;
        }
        match id {
            "saved-searches" => self.open_saved_searches(),
            "save-search" if SearchQuery::new(&self.query).is_ok() => {
                self.save_name.clear();
                self.saving_name = true;
                self.status = None;
            }
            _ => return false,
        }
        true
    }

    fn render_inspector(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
        if self.editing || self.saving_name || self.saved_searches_open {
            return false;
        }
        let Some(result) = self.results.get(self.selected) else {
            return false;
        };
        let lines = vec![
            Line::from(result.item().title().to_owned())
                .style(Style::default().add_modifier(Modifier::BOLD)),
            Line::from(result.item().reference().to_string()),
            Line::from(result.excerpt().to_owned()),
        ];
        frame.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }),
            area,
        );
        true
    }
}
