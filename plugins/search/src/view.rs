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
use tuilane_plugin_api::search::{SearchCommand, SearchCommandResult, SearchQuery, SearchResult};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{FocusArea, View};

/// Search workspace items through the shared host command path.
pub struct SearchView {
    descriptor: ViewDescriptor,
    commands: Rc<RefCell<CommandDispatcher>>,
    query: String,
    editing: bool,
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
            editing: false,
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
        let command = SearchCommand::new(query);
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

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = vec![Line::from(if self.editing {
            format!("/{}", self.query)
        } else if self.query.is_empty() {
            "Press / to search workspace items".to_owned()
        } else {
            format!("Query: {}", self.query)
        })];
        lines.extend(self.result_lines());
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

    fn render_inspector(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
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
