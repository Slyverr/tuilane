use std::{cell::RefCell, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use tuilane_core::CommandDispatcher;
use tuilane_plugin_api::search::{
    ListSavedSearchesCommand, SaveSavedSearchCommand, SavedSearch, SavedSearchName, SearchCommand,
    SearchCommandResult, SearchQuery, SearchResult,
};
use tuilane_plugin_api::{
    DashboardItem, DashboardLayout, DashboardOption, DashboardOptionKind, DashboardOptionValue,
    DashboardSize, DashboardTone, DashboardWidget, DashboardWidgetContent,
    DashboardWidgetDescriptor, DashboardWidgetId, ViewDescriptor, ViewId,
};
use tuilane_ui::{
    BrowserDetails, BrowserHitMap, BrowserItem, BrowserTone, FocusArea, Keybinding, ThemePreset,
    View, ViewAction, ViewDialog, render_browser, render_empty_state, select_browser_row,
};

fn search_icon(kind: &str) -> &'static str {
    match kind {
        "task" => tuilane_ui::icons::TASKS,
        "project" => tuilane_ui::icons::PROJECTS,
        "note" => tuilane_ui::icons::NOTES,
        "idea" => tuilane_ui::icons::IDEAS,
        _ => tuilane_ui::icons::DEFAULT,
    }
}

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
    browser_hits: BrowserHitMap,
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
            browser_hits: BrowserHitMap::default(),
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
}

impl View for SearchView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn icon(&self) -> &'static str {
        tuilane_ui::icons::SEARCH
    }

    fn keybindings(&self, focus: FocusArea) -> Vec<Keybinding> {
        if focus != FocusArea::Content {
            return Vec::new();
        }

        if self.saving_name {
            return vec![
                Keybinding::new("Save", "Enter"),
                Keybinding::new("Cancel", "Esc"),
            ];
        }
        if self.saved_searches_open {
            return vec![
                Keybinding::new("Move", "↑/↓, j/k").with_priority(10),
                Keybinding::new("Run", "Enter").with_priority(90),
                Keybinding::new("Close", "Esc").with_priority(30),
            ];
        }
        if self.editing {
            return vec![
                Keybinding::new("Run", "Enter"),
                Keybinding::new("Cancel", "Esc"),
                Keybinding::new("Query", "Type"),
            ];
        }

        vec![
            Keybinding::new("Search", "/").with_priority(90),
            Keybinding::new("Results", "↑/↓, j/k").with_priority(10),
            Keybinding::new("Filter", "f").with_priority(80),
        ]
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.render_with_theme(frame, area, ThemePreset::default());
    }

    fn render_with_theme(&mut self, frame: &mut Frame<'_>, area: Rect, theme: ThemePreset) {
        if self.saved_searches_open {
            let items = self
                .saved_searches
                .iter()
                .map(|saved| {
                    BrowserItem::new(saved.name().as_str())
                        .with_icon(tuilane_ui::icons::SEARCH)
                        .with_summary(saved.query().as_str())
                        .with_badge(saved.kind().unwrap_or("All"), BrowserTone::Accent)
                })
                .collect::<Vec<_>>();
            let details = self.saved_searches.get(self.saved_selected).map(|saved| {
                BrowserDetails::new(saved.name().as_str())
                    .with_eyebrow("SAVED SEARCH")
                    .with_field("Query", saved.query().as_str())
                    .with_field("Type", saved.kind().unwrap_or("All items"))
                    .with_summary("Press Enter to run this search.")
            });
            render_browser(
                frame,
                area,
                theme,
                &items,
                Some(self.saved_selected),
                details.as_ref(),
                "No saved searches",
                "Save a search from the command palette to keep it for later.",
                &mut self.browser_hits,
            );
            return;
        }

        if self.query.is_empty() && !self.editing {
            self.browser_hits.clear();
            render_empty_state(
                frame,
                area,
                "Search your workspace",
                "Press / to find tasks, projects, notes, and ideas.",
            );
            return;
        }

        let query = if self.editing {
            format!("/{}_", self.query)
        } else {
            self.query.clone()
        };
        let filter = self.kind.as_deref().unwrap_or("All items");
        let count = self.results.len();
        let header = Line::from(vec![
            Span::styled(
                format!("{}  ", tuilane_ui::icons::SEARCH),
                theme.accent_style(),
            ),
            Span::styled(query, theme.header_style().add_modifier(Modifier::BOLD)),
            Span::raw("   ·   "),
            Span::styled(filter, theme.muted_style()),
            Span::raw(if self.editing {
                String::new()
            } else {
                format!("   ·   {count} matches")
            }),
        ]);
        let header_area = Rect::new(area.x, area.y, area.width, 1.min(area.height));
        frame.render_widget(Paragraph::new(header), header_area);
        let body = Rect::new(
            area.x,
            area.y.saturating_add(2),
            area.width,
            area.height.saturating_sub(2),
        );

        if self.results.is_empty() {
            self.browser_hits.clear();
            render_empty_state(
                frame,
                body,
                if self.editing {
                    "Keep typing"
                } else {
                    "No matches"
                },
                if self.editing {
                    "Run the search with Enter."
                } else {
                    "Try a different query or item filter."
                },
            );
            return;
        }

        let items = self
            .results
            .iter()
            .map(|result| {
                let kind = result.item().reference().kind();
                BrowserItem::new(result.item().title())
                    .with_icon(search_icon(kind))
                    .with_summary(result.excerpt())
                    .with_badge(kind, BrowserTone::Accent)
            })
            .collect::<Vec<_>>();
        let details = self.results.get(self.selected).map(|result| {
            BrowserDetails::new(result.item().title())
                .with_eyebrow(format!("{} RESULT", result.item().reference().kind()))
                .with_field("Reference", result.item().reference().to_string())
                .with_field("Query", self.query.as_str())
                .with_body(result.excerpt())
        });
        render_browser(
            frame,
            body,
            theme,
            &items,
            Some(self.selected),
            details.as_ref(),
            "No matches",
            "Try a different query or item filter.",
            &mut self.browser_hits,
        );
    }

    fn handle_mouse(&mut self, event: crossterm::event::MouseEvent, focus: FocusArea) -> bool {
        if focus != FocusArea::Content || self.editing || self.saving_name {
            return false;
        }
        if self.saved_searches_open {
            let mut selected = Some(self.saved_selected);
            if select_browser_row(&self.browser_hits, &mut selected, event) {
                self.saved_selected = selected.unwrap_or_default();
                return true;
            }
        } else {
            let mut selected = Some(self.selected);
            if select_browser_row(&self.browser_hits, &mut selected, event) {
                self.selected = selected.unwrap_or_default();
                return true;
            }
        }
        false
    }

    fn dashboard_widgets(&mut self, layout: &DashboardLayout) -> Vec<DashboardWidget> {
        let id = DashboardWidgetId::new("tuilane.search.saved-searches")
            .expect("static dashboard widget identifier is valid");
        let descriptor = DashboardWidgetDescriptor::new(
            id.clone(),
            "Saved searches",
            "Quick access to reusable workspace queries",
            DashboardSize::Compact,
            vec![DashboardOption::new(
                "limit",
                "Visible searches",
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
        let content = self
            .commands
            .try_borrow_mut()
            .map_err(|_| "The command dispatcher is busy.".to_owned())
            .and_then(|mut commands| {
                commands
                    .execute(ListSavedSearchesCommand)
                    .map_err(|error| error.to_string())
            })
            .map_or_else(
                |error| DashboardWidgetContent::Message {
                    title: "Saved searches unavailable".into(),
                    detail: error.into_boxed_str(),
                    tone: DashboardTone::Warning,
                },
                |searches: Vec<SavedSearch>| DashboardWidgetContent::List {
                    items: searches
                        .iter()
                        .take(limit)
                        .map(|search| {
                            DashboardItem::new(
                                search.name().as_str(),
                                Some(
                                    format!(
                                        "{} · {}",
                                        search.kind().unwrap_or("all"),
                                        search.query().as_str()
                                    )
                                    .into_boxed_str(),
                                ),
                            )
                        })
                        .collect(),
                    empty_message: "No saved searches".into(),
                },
            );
        vec![DashboardWidget::new(descriptor, content)]
    }

    fn open_dashboard_widget(&mut self, id: &DashboardWidgetId) {
        if id.as_str() == "tuilane.search.saved-searches" {
            self.open_saved_searches();
        }
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
                    self.status = None;
                }
                KeyCode::Enter => self.save_current_search(),
                KeyCode::Backspace => {
                    self.status = None;
                    self.save_name.pop();
                }
                KeyCode::Char(character)
                    if !key.modifiers.intersects(
                        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                    ) && self.save_name.chars().count() < 64 =>
                {
                    self.status = None;
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
                KeyCode::Char('k') => {
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
                KeyCode::Char('j') => {
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
                    self.results.clear();
                    self.selected = 0;
                }
                KeyCode::Char(character)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && self.query.chars().count() < 256 =>
                {
                    self.query.push(character);
                    self.results.clear();
                    self.selected = 0;
                }
                _ => {}
            }
            return true;
        }
        match key.code {
            KeyCode::Char('/') => {
                self.query.clear();
                self.results.clear();
                self.selected = 0;
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

    fn dialog(&self) -> Option<ViewDialog> {
        self.saving_name.then(|| ViewDialog::Input {
            title: "Save search".to_owned(),
            label: "Name".to_owned(),
            value: self.save_name.clone(),
            error: self.status.clone(),
        })
    }

    fn take_notification(&mut self) -> Option<String> {
        if self.saving_name {
            None
        } else {
            self.status.take()
        }
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
