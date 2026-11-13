use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_core::{CommandDispatcher, EventBus, ServiceBroker};
use tuilane_notes::{NoteError, NotesPlugin, NotesView};
use tuilane_plugin_api::items::{ItemRef, ItemRemovedV1};
use tuilane_plugin_api::notes::{
    Markdown, NoteByIdV1, NoteCommand, NoteCommandResult, NoteId, NoteTitle,
};
use tuilane_storage::SqliteDatabase;
use tuilane_ui::Workspace;
use uuid::Uuid;

struct TestDatabase {
    directory: PathBuf,
}

impl TestDatabase {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("tuilane-notes-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("create temporary database directory");
        Self { directory }
    }

    fn open(&self) -> SqliteDatabase {
        SqliteDatabase::open(self.directory.join("notes.sqlite3")).expect("open temporary database")
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).expect("remove temporary database directory");
    }
}

struct NoteHost {
    commands: CommandDispatcher,
    services: ServiceBroker,
    events: EventBus,
}

fn note_host(database: SqliteDatabase) -> NoteHost {
    let plugin = NotesPlugin::new(database).expect("initialize notes schema");
    let mut commands = CommandDispatcher::new();
    let mut services = ServiceBroker::new();
    let events = EventBus::new();
    plugin
        .register(&mut commands, &mut services, events.clone())
        .expect("register note contracts");
    NoteHost {
        commands,
        services,
        events,
    }
}

fn create_note(
    commands: &mut CommandDispatcher,
    title: &str,
    body: &str,
) -> tuilane_plugin_api::notes::Note {
    match commands
        .execute(NoteCommand::Create {
            title: NoteTitle::new(title).expect("valid note title"),
            body: Markdown::new(body).expect("valid Markdown"),
        })
        .expect("create note")
    {
        NoteCommandResult::Note(note) => note,
        _ => panic!("create returns a note"),
    }
}

#[test]
fn note_content_and_renames_survive_reopening_the_database() {
    let file = TestDatabase::new();
    let id = {
        let mut host = note_host(file.open());
        let note = create_note(&mut host.commands, "Research", "# Finding\n\nDetails");
        host.commands
            .execute(NoteCommand::Rename {
                id: note.id().clone(),
                title: NoteTitle::new("Research notes").expect("valid note title"),
            })
            .expect("rename note");
        host.commands
            .execute(NoteCommand::SetBody {
                id: note.id().clone(),
                body: Markdown::new("Updated **Markdown**").expect("valid Markdown"),
            })
            .expect("update note body");
        note.id().clone()
    };

    let mut host = note_host(file.open());
    let result = host
        .commands
        .execute(NoteCommand::Get { id })
        .expect("read persisted note");

    assert!(matches!(result, NoteCommandResult::Note(ref note)
        if note.title().as_str() == "Research notes"
            && note.body().as_str() == "Updated **Markdown**"));
}

#[test]
fn note_item_lookup_returns_stable_display_metadata() {
    let file = TestDatabase::new();
    let mut host = note_host(file.open());
    let note = create_note(&mut host.commands, "Meeting notes", "Decisions");

    let summary = host
        .services
        .call(NoteByIdV1::new(note.id().clone()))
        .expect("query note item service")
        .expect("note exists");

    assert_eq!(
        summary.reference(),
        &ItemRef::new("note", note.id().as_str()).expect("valid item reference")
    );
    assert_eq!(summary.title(), "Meeting notes");
}

#[test]
fn successful_note_deletion_publishes_a_stable_removal_event() {
    let file = TestDatabase::new();
    let mut host = note_host(file.open());
    let removed = Rc::new(RefCell::new(None));
    let event_target = Rc::clone(&removed);
    let _subscription = host.events.subscribe(move |event: &ItemRemovedV1| {
        *event_target.borrow_mut() = Some(event.item().clone());
    });
    let note = create_note(&mut host.commands, "Remove me", "");

    host.commands
        .execute(NoteCommand::Delete {
            id: note.id().clone(),
        })
        .expect("delete note");

    assert_eq!(
        *removed.borrow(),
        Some(ItemRef::new("note", note.id().as_str()).expect("valid item reference"))
    );
}

#[test]
fn missing_notes_return_a_domain_error() {
    let file = TestDatabase::new();
    let mut host = note_host(file.open());
    let id = NoteId::new("00000000-0000-4000-8000-000000000001").expect("valid note ID");

    let error = host
        .commands
        .execute(NoteCommand::Get { id })
        .expect_err("unknown note is rejected");

    assert!(error.to_string().contains("was not found"));
}

#[test]
fn notes_view_creates_records_through_the_host_command_path() {
    let file = TestDatabase::new();
    let host = note_host(file.open());
    let commands = Rc::new(RefCell::new(host.commands));
    let mut workspace = Workspace::new();
    workspace
        .register_view(NotesView::new(commands, &host.events))
        .expect("register Notes view");

    workspace.handle_key(key(KeyCode::Char('c')));
    for character in "Draft".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    let text = render_text(&mut workspace);

    assert!(text.contains("Draft"));
    assert!(text.contains("empty Markdown"));
}

#[test]
fn newer_note_schema_versions_are_rejected() {
    let file = TestDatabase::new();
    let database = file.open();
    database
        .with_connection(|connection| {
            connection.execute_batch(
                "CREATE TABLE tuilane_notes_migrations (version INTEGER PRIMARY KEY NOT NULL);",
            )?;
            connection.execute(
                "INSERT INTO tuilane_notes_migrations (version) VALUES (?1)",
                [99],
            )?;
            Ok(())
        })
        .expect("prepare newer schema version");

    assert!(matches!(
        NotesPlugin::new(database),
        Err(NoteError::UnsupportedSchemaVersion(99))
    ));
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_text(workspace: &mut Workspace) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("create test terminal");
    terminal
        .draw(|frame| workspace.render(frame))
        .expect("render Notes workspace");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
