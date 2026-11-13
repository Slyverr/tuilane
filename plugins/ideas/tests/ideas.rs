use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_core::{CommandDispatcher, EventBus, ServiceBroker};
use tuilane_ideas::{IdeaError, IdeasPlugin, IdeasView};
use tuilane_plugin_api::ideas::{IdeaByIdV1, IdeaCommand, IdeaCommandResult, IdeaId, IdeaTitle};
use tuilane_plugin_api::items::{ItemRef, ItemRemovedV1};
use tuilane_plugin_api::search::IdeaSearchDocumentsV1;
use tuilane_storage::SqliteDatabase;
use tuilane_ui::Workspace;
use uuid::Uuid;

struct TestDatabase(PathBuf);

impl TestDatabase {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("tuilane-ideas-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("create temporary database directory");
        Self(directory)
    }

    fn open(&self) -> SqliteDatabase {
        SqliteDatabase::open(self.0.join("ideas.sqlite3")).expect("open temporary database")
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove temporary database directory");
    }
}

struct IdeaHost {
    commands: CommandDispatcher,
    services: ServiceBroker,
    events: EventBus,
}

fn idea_host(database: SqliteDatabase) -> IdeaHost {
    let plugin = IdeasPlugin::new(database).expect("initialize ideas schema");
    let mut commands = CommandDispatcher::new();
    let mut services = ServiceBroker::new();
    let events = EventBus::new();
    plugin
        .register(&mut commands, &mut services, events.clone())
        .expect("register idea contracts");
    IdeaHost {
        commands,
        services,
        events,
    }
}

fn create_idea(commands: &mut CommandDispatcher, title: &str) -> tuilane_plugin_api::ideas::Idea {
    match commands
        .execute(IdeaCommand::Create {
            title: IdeaTitle::new(title).expect("valid idea title"),
        })
        .expect("create idea")
    {
        IdeaCommandResult::Idea(idea) => idea,
        _ => panic!("create returns an idea"),
    }
}

#[test]
fn ideas_survive_database_reopening_and_can_be_renamed() {
    let file = TestDatabase::new();
    let id = {
        let mut host = idea_host(file.open());
        let idea = create_idea(&mut host.commands, "Track reading notes");
        host.commands
            .execute(IdeaCommand::Rename {
                id: idea.id().clone(),
                title: IdeaTitle::new("Connect reading notes").expect("valid idea title"),
            })
            .expect("rename idea");
        idea.id().clone()
    };

    let mut host = idea_host(file.open());
    let result = host
        .commands
        .execute(IdeaCommand::Get { id })
        .expect("read persisted idea");

    assert!(matches!(result, IdeaCommandResult::Idea(ref idea)
        if idea.title().as_str() == "Connect reading notes"));
}

#[test]
fn idea_item_lookup_returns_stable_display_metadata() {
    let file = TestDatabase::new();
    let mut host = idea_host(file.open());
    let idea = create_idea(&mut host.commands, "Build a weekly review");

    let summary = host
        .services
        .call(IdeaByIdV1::new(idea.id().clone()))
        .expect("query idea item service")
        .expect("idea exists");

    assert_eq!(
        summary.reference(),
        &ItemRef::new("idea", idea.id().as_str()).expect("valid item reference")
    );
    assert_eq!(summary.title(), "Build a weekly review");
}

#[test]
fn idea_search_service_exposes_idea_titles() {
    let file = TestDatabase::new();
    let mut host = idea_host(file.open());
    let idea = create_idea(&mut host.commands, "Map release feedback");

    let documents = host
        .services
        .call(IdeaSearchDocumentsV1)
        .expect("query idea search documents");

    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].item().title(), "Map release feedback");
    assert_eq!(
        documents[0].item().reference(),
        &ItemRef::new("idea", idea.id().as_str()).expect("valid idea reference")
    );
}

#[test]
fn successful_idea_deletion_publishes_a_stable_removal_event() {
    let file = TestDatabase::new();
    let mut host = idea_host(file.open());
    let removed = Rc::new(RefCell::new(None));
    let event_target = Rc::clone(&removed);
    let _subscription = host.events.subscribe(move |event: &ItemRemovedV1| {
        *event_target.borrow_mut() = Some(event.item().clone());
    });
    let idea = create_idea(&mut host.commands, "Remove me");

    host.commands
        .execute(IdeaCommand::Delete {
            id: idea.id().clone(),
        })
        .expect("delete idea");

    assert_eq!(
        *removed.borrow(),
        Some(ItemRef::new("idea", idea.id().as_str()).expect("valid item reference"))
    );
}

#[test]
fn missing_ideas_return_a_domain_error() {
    let file = TestDatabase::new();
    let mut host = idea_host(file.open());
    let id = IdeaId::new("00000000-0000-4000-8000-000000000001").expect("valid idea ID");

    let error = host
        .commands
        .execute(IdeaCommand::Get { id })
        .expect_err("unknown idea is rejected");

    assert!(error.to_string().contains("was not found"));
}

#[test]
fn ideas_view_captures_records_through_the_host_command_path() {
    let file = TestDatabase::new();
    let host = idea_host(file.open());
    let commands = Rc::new(RefCell::new(host.commands));
    let mut workspace = Workspace::new();
    workspace
        .register_view(IdeasView::new(commands, &host.events))
        .expect("register Ideas view");

    activate_palette_action(&mut workspace, "Capture idea");
    for character in "Prototype offline capture".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    let text = render_text(&mut workspace);

    assert!(text.contains("Prototype offline capture"));
}

#[test]
fn newer_idea_schema_versions_are_rejected() {
    let file = TestDatabase::new();
    let database = file.open();
    database
        .with_connection(|connection| {
            connection.execute_batch(
                "CREATE TABLE tuilane_ideas_migrations (version INTEGER PRIMARY KEY NOT NULL);",
            )?;
            connection.execute(
                "INSERT INTO tuilane_ideas_migrations (version) VALUES (?1)",
                [99],
            )?;
            Ok(())
        })
        .expect("prepare newer schema version");

    assert!(matches!(
        IdeasPlugin::new(database),
        Err(IdeaError::UnsupportedSchemaVersion(99))
    ));
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn activate_palette_action(workspace: &mut Workspace, query: &str) {
    workspace.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    for character in query.chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
}

fn render_text(workspace: &mut Workspace) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("create test terminal");
    terminal
        .draw(|frame| workspace.render(frame))
        .expect("render Ideas workspace");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
