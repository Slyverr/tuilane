use std::{cell::RefCell, rc::Rc};

use tuilane_core::{CommandDispatcher, EventBus, ServiceBroker};
use tuilane_plugin_api::{
    items::{ItemRef, ItemSummary, SearchDocument},
    notes::NotesChangedV1,
    search::{
        IdeaSearchDocumentsV1, NoteSearchDocumentsV1, ProjectSearchDocumentsV1, SearchCommand,
        SearchCommandResult, SearchQuery, TaskSearchDocumentsV1,
    },
};
use tuilane_search::SearchPlugin;
use tuilane_storage::SqliteDatabase;

fn document(kind: &str, id: &str, title: &str, content: &str) -> SearchDocument {
    SearchDocument::new(
        ItemSummary::new(ItemRef::new(kind, id).unwrap(), title).unwrap(),
        content,
    )
}

fn search_command(query: &str) -> SearchCommand {
    SearchCommand::new(SearchQuery::new(query).unwrap())
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
