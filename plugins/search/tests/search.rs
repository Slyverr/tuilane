use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_core::{CommandDispatcher, EventBus, ServiceBroker};
use tuilane_plugin_api::{
    items::{ItemRef, ItemSummary, SearchDocument},
    notes::NotesChangedV1,
    search::{
        DeleteSavedSearchCommand, IdeaSearchDocumentsV1, ListSavedSearchesCommand,
        NoteSearchDocumentsV1, ProjectSearchDocumentsV1, SaveSavedSearchCommand, SavedSearch,
        SavedSearchName, SearchCommand, SearchCommandResult, SearchQuery, SearchResult,
        TaskSearchDocumentsV1,
    },
};
use tuilane_search::SearchPlugin;
use tuilane_search::SearchView;
use tuilane_storage::SqliteDatabase;
use tuilane_ui::{FocusArea, Workspace};

fn document(kind: &str, id: &str, title: &str, content: &str) -> SearchDocument {
    SearchDocument::new(
        ItemSummary::new(ItemRef::new(kind, id).unwrap(), title).unwrap(),
        content,
    )
}

fn search_command(query: &str) -> SearchCommand {
    SearchCommand::new(SearchQuery::new(query).unwrap())
}

struct TestDatabase(PathBuf);

impl TestDatabase {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "tuilane-search-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn open(&self) -> SqliteDatabase {
        SqliteDatabase::open(self.0.join("search.sqlite3")).unwrap()
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn empty_search_services() -> Rc<RefCell<ServiceBroker>> {
    let mut services = ServiceBroker::new();
    services
        .register(|_: TaskSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    services
        .register(|_: ProjectSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    services
        .register(|_: NoteSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    services
        .register(|_: IdeaSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    Rc::new(RefCell::new(services))
}

#[test]
fn search_indexes_owner_documents_and_filters_by_item_kind() {
    let notes = Rc::new(RefCell::new(vec![document(
        "note",
        "note-1",
        "Research notes",
        "Distributed systems need durable search",
    )]));
    let tasks = Rc::new(RefCell::new(vec![document(
        "task",
        "task-1",
        "Write tests",
        "open 2026-12-01",
    )]));
    let mut services = ServiceBroker::new();
    let provider = Rc::clone(&tasks);
    services
        .register(move |_: TaskSearchDocumentsV1| Ok::<_, &'static str>(provider.borrow().clone()))
        .unwrap();
    services
        .register(|_: ProjectSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    let note_provider = Rc::clone(&notes);
    services
        .register(move |_: NoteSearchDocumentsV1| {
            Ok::<_, &'static str>(note_provider.borrow().clone())
        })
        .unwrap();
    services
        .register(|_: IdeaSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();

    let database = SqliteDatabase::open_in_memory().unwrap();
    let services = Rc::new(RefCell::new(services));
    let mut commands = CommandDispatcher::new();
    let events = EventBus::new();
    let _plugin = SearchPlugin::register(database, services, &mut commands, &events).unwrap();

    let all = commands
        .execute(search_command("distributed systems"))
        .unwrap();
    let SearchCommandResult::Results(all) = all;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].item().reference().kind(), "note");
    assert!(all[0].excerpt().contains("[Distributed]"));

    let task_query = search_command("open").with_kind("task").unwrap();
    let SearchCommandResult::Results(tasks) = commands.execute(task_query).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].item().reference().id(), "task-1");
}

#[test]
fn feature_change_events_refresh_the_durable_index() {
    let notes = Rc::new(RefCell::new(vec![document(
        "note",
        "note-1",
        "First draft",
        "old content",
    )]));
    let mut services = ServiceBroker::new();
    for result in [
        services.register(|_: TaskSearchDocumentsV1| {
            Ok::<_, &'static str>(Vec::<SearchDocument>::new())
        }),
        services.register(|_: ProjectSearchDocumentsV1| {
            Ok::<_, &'static str>(Vec::<SearchDocument>::new())
        }),
    ] {
        result.unwrap();
    }
    let note_provider = Rc::clone(&notes);
    services
        .register(move |_: NoteSearchDocumentsV1| {
            Ok::<_, &'static str>(note_provider.borrow().clone())
        })
        .unwrap();
    services
        .register(|_: IdeaSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();

    let mut commands = CommandDispatcher::new();
    let events = EventBus::new();
    let _plugin = SearchPlugin::register(
        SqliteDatabase::open_in_memory().unwrap(),
        Rc::new(RefCell::new(services)),
        &mut commands,
        &events,
    )
    .unwrap();
    let SearchCommandResult::Results(before) = commands.execute(search_command("old")).unwrap();
    assert_eq!(before.len(), 1);

    *notes.borrow_mut() = vec![document("note", "note-1", "Revised draft", "new content")];
    events.publish(NotesChangedV1);

    let SearchCommandResult::Results(old) = commands.execute(search_command("old")).unwrap();
    let SearchCommandResult::Results(new) = commands.execute(search_command("new")).unwrap();
    assert!(old.is_empty());
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].item().title(), "Revised draft");
}

#[test]
fn punctuation_only_queries_return_no_matches_without_fts_errors() {
    let mut services = ServiceBroker::new();
    services
        .register(|_: TaskSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    services
        .register(|_: ProjectSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    services
        .register(|_: NoteSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    services
        .register(|_: IdeaSearchDocumentsV1| Ok::<_, &'static str>(Vec::<SearchDocument>::new()))
        .unwrap();
    let mut commands = CommandDispatcher::new();
    let _plugin = SearchPlugin::register(
        SqliteDatabase::open_in_memory().unwrap(),
        Rc::new(RefCell::new(services)),
        &mut commands,
        &EventBus::new(),
    )
    .unwrap();

    let SearchCommandResult::Results(results) = commands.execute(search_command("! ? *")).unwrap();
    assert!(results.is_empty());
}

#[test]
fn saved_searches_upgrade_the_index_schema_and_survive_reopening() {
    let file = TestDatabase::new();
    let saved = SavedSearch::new(
        SavedSearchName::new("Open tasks").unwrap(),
        SearchQuery::new("review due date").unwrap(),
        Some("task"),
    )
    .unwrap();

    {
        let database = file.open();
        database
            .with_connection(|connection| {
                connection.execute_batch(
                    "CREATE TABLE tuilane_search_migrations (version INTEGER PRIMARY KEY NOT NULL);
                     INSERT INTO tuilane_search_migrations (version) VALUES (1);
                     CREATE VIRTUAL TABLE tuilane_search_index USING fts5(item_ref UNINDEXED, kind UNINDEXED, title, content);",
                )?;
                Ok(())
            })
            .unwrap();
        let services = empty_search_services();
        let mut commands = CommandDispatcher::new();
        let events = EventBus::new();
        let _plugin = SearchPlugin::register(database, services, &mut commands, &events).unwrap();

        commands
            .execute(SaveSavedSearchCommand::new(saved.clone()))
            .unwrap();
        let duplicate = commands
            .execute(SaveSavedSearchCommand::new(saved.clone()))
            .unwrap_err();
        assert!(duplicate.to_string().contains("already exists"));
    }

    let database = file.open();
    let services = empty_search_services();
    let mut commands = CommandDispatcher::new();
    let events = EventBus::new();
    let _plugin = SearchPlugin::register(database, services, &mut commands, &events).unwrap();
    let saved_searches = commands.execute(ListSavedSearchesCommand).unwrap();
    assert_eq!(saved_searches, vec![saved.clone()]);

    assert!(
        commands
            .execute(DeleteSavedSearchCommand::new(saved.name().clone()))
            .unwrap()
    );
    assert!(
        !commands
            .execute(DeleteSavedSearchCommand::new(saved.name().clone()))
            .unwrap()
    );
    assert!(
        commands
            .execute(ListSavedSearchesCommand)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn search_view_routes_queries_and_inspects_the_selected_result() {
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register(move |command: SearchCommand| {
            assert_eq!(command.query().as_str(), "signal");
            Ok::<_, &'static str>(SearchCommandResult::Results(vec![
                tuilane_plugin_api::search::SearchResult::new(
                    ItemSummary::new(ItemRef::new("note", "note-1").unwrap(), "Signal processing")
                        .unwrap(),
                    "A [signal] found in the sample",
                ),
            ]))
        })
        .unwrap();
    let commands = Rc::new(RefCell::new(dispatcher));
    let mut workspace = Workspace::new();
    workspace.register_view(SearchView::new(commands)).unwrap();

    workspace.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    for character in "signal".chars() {
        workspace.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    workspace.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    workspace.toggle_inspector();
    assert_eq!(workspace.focus(), FocusArea::Inspector);

    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let output = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(output.contains("Query: signal"), "{output}");
    assert!(output.contains("Signal processing"), "{output}");
    assert!(output.contains("A [signal] found in the"), "{output}");
    assert!(output.contains("sample"), "{output}");
}

#[test]
fn search_view_saves_and_runs_named_filtered_views_from_the_palette() {
    let searches = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&searches);
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register(move |command: SearchCommand| {
            observed.borrow_mut().push((
                command.query().as_str().to_owned(),
                command.kind().map(str::to_owned),
            ));
            Ok::<_, &'static str>(SearchCommandResult::Results(vec![SearchResult::new(
                ItemSummary::new(ItemRef::new("task", "task-1").unwrap(), "Review signal").unwrap(),
                "A [signal] found in a task",
            )]))
        })
        .unwrap();
    let saved_searches = Rc::new(RefCell::new(Vec::<SavedSearch>::new()));
    let save_target = Rc::clone(&saved_searches);
    dispatcher
        .register(move |command: SaveSavedSearchCommand| {
            let saved = command.search().clone();
            save_target.borrow_mut().push(saved.clone());
            Ok::<_, &'static str>(saved)
        })
        .unwrap();
    let list_target = Rc::clone(&saved_searches);
    dispatcher
        .register(move |_: ListSavedSearchesCommand| {
            Ok::<_, &'static str>(list_target.borrow().clone())
        })
        .unwrap();
    let mut workspace = Workspace::new();
    workspace
        .register_view(SearchView::new(Rc::new(RefCell::new(dispatcher))))
        .unwrap();

    workspace.handle_key(key(KeyCode::Char('f')));
    workspace.handle_key(key(KeyCode::Char('/')));
    type_text(&mut workspace, "signal");
    workspace.handle_key(key(KeyCode::Enter));
    activate_palette_action(&mut workspace, "Save current search");
    type_text(&mut workspace, "Signal tasks");
    workspace.handle_key(key(KeyCode::Enter));

    assert_eq!(saved_searches.borrow().len(), 1);
    assert_eq!(saved_searches.borrow()[0].kind(), Some("task"));

    activate_palette_action(&mut workspace, "Browse saved searches");
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal.draw(|frame| workspace.render(frame)).unwrap();
    let output = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(output.contains("Signal tasks"));
    assert!(output.contains("[task] signal"));
    workspace.handle_key(key(KeyCode::Enter));

    assert_eq!(
        *searches.borrow(),
        vec![
            ("signal".to_owned(), Some("task".to_owned())),
            ("signal".to_owned(), Some("task".to_owned()))
        ]
    );
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_text(workspace: &mut Workspace, value: &str) {
    for character in value.chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
}

fn activate_palette_action(workspace: &mut Workspace, query: &str) {
    workspace.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    type_text(workspace, query);
    workspace.handle_key(key(KeyCode::Enter));
}
