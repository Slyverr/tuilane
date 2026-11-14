use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_core::{CommandDispatcher, EventBus, ServiceBroker};
use tuilane_links::{LinkError, LinksPlugin, LinksView};
use tuilane_plugin_api::{
    items::{ItemRef, ItemRemovedV1, ItemSummary},
    links::{
        ExternalReference, LinkCommand, LinkCommandResult, LinksChangedV1, ReferencesForItemV1,
    },
    notes::NoteByIdV1,
    projects::ProjectByIdV1,
    tasks::TaskByIdV1,
};
use tuilane_storage::SqliteDatabase;
use tuilane_ui::{FocusArea, View, Workspace};

const IDEA: &str = "idea:00000000-0000-4000-8000-000000000001";
const NOTE: &str = "note:00000000-0000-4000-8000-000000000001";
const TASK: &str = "task:00000000-0000-4000-8000-000000000001";
const PROJECT: &str = "project:00000000-0000-4000-8000-000000000001";
const MISSING_ID: &str = "00000000-0000-4000-8000-000000000002";

struct TestDatabase(PathBuf);

impl TestDatabase {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tuilane-links-test-{}-{}",
            std::process::id(),
            randless_id()
        ));
        fs::create_dir(&path).expect("create temporary database directory");
        Self(path)
    }

    fn open(&self) -> SqliteDatabase {
        SqliteDatabase::open(self.0.join("links.sqlite3")).expect("open temporary database")
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove temporary database directory");
    }
}

fn randless_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

struct LinkHost {
    commands: CommandDispatcher,
    services: Rc<RefCell<ServiceBroker>>,
    events: EventBus,
}

fn link_host(database: SqliteDatabase) -> LinkHost {
    let services = Rc::new(RefCell::new(ServiceBroker::new()));
    register_item_services(&mut services.borrow_mut());
    let events = EventBus::new();
    let mut commands = CommandDispatcher::new();
    LinksPlugin::new(database)
        .expect("initialize links schema")
        .register(&mut commands, services.clone(), events.clone())
        .expect("register link contracts");
    LinkHost {
        commands,
        services,
        events,
    }
}

fn register_item_services(services: &mut ServiceBroker) {
    services
        .register(|request: TaskByIdV1| {
            Ok::<_, &'static str>(item_summary("task", request.id().as_str(), "Review task"))
        })
        .unwrap();
    services
        .register(|request: ProjectByIdV1| {
            Ok::<_, &'static str>(item_summary(
                "project",
                request.id().as_str(),
                "Roadmap project",
            ))
        })
        .unwrap();
    services
        .register(|request: NoteByIdV1| {
            Ok::<_, &'static str>(item_summary("note", request.id().as_str(), "Research note"))
        })
        .unwrap();
    services
        .register(|request: tuilane_plugin_api::ideas::IdeaByIdV1| {
            Ok::<_, &'static str>(item_summary(
                "idea",
                request.id().as_str(),
                "Connected idea",
            ))
        })
        .unwrap();
}

fn item_summary(kind: &str, id: &str, title: &str) -> Option<ItemSummary> {
    if id == MISSING_ID {
        return None;
    }
    Some(ItemSummary::new(ItemRef::new(kind, id).unwrap(), title).unwrap())
}

fn item(reference: &str) -> ItemRef {
    reference.parse().expect("valid item reference")
}

#[test]
fn item_links_and_backlinks_survive_reopening_the_database() {
    let file = TestDatabase::new();
    {
        let mut host = link_host(file.open());
        host.commands
            .execute(LinkCommand::Add {
                source: item(IDEA),
                target: item(NOTE),
            })
            .expect("add idea to note link");
        host.commands
            .execute(LinkCommand::Add {
                source: item(IDEA),
                target: item(TASK),
            })
            .expect("add idea to task link");
    }

    let mut host = link_host(file.open());
    let outgoing = host
        .commands
        .execute(LinkCommand::Outgoing { item: item(IDEA) })
        .expect("list idea links");
    let backlinks = host
        .commands
        .execute(LinkCommand::Backlinks { item: item(NOTE) })
        .expect("list note backlinks");

    assert!(matches!(outgoing, LinkCommandResult::Items(items)
        if items.iter().map(ItemSummary::title).collect::<Vec<_>>() == ["Research note", "Review task"]));
    assert!(matches!(backlinks, LinkCommandResult::Items(items)
        if items.len() == 1 && items[0].reference() == &item(IDEA)));
}

#[test]
fn external_file_and_url_references_survive_reopening_the_database() {
    let file = TestDatabase::new();
    let path = ExternalReference::file("research/meeting notes.md").unwrap();
    let url = ExternalReference::url("https://example.org/research").unwrap();
    {
        let mut host = link_host(file.open());
        host.commands
            .execute(LinkCommand::AddReference {
                owner: item(IDEA),
                reference: path.clone(),
            })
            .expect("attach file reference");
        host.commands
            .execute(LinkCommand::AddReference {
                owner: item(IDEA),
                reference: url.clone(),
            })
            .expect("attach URL reference");
    }

    let mut host = link_host(file.open());
    let references = host
        .commands
        .execute(LinkCommand::References { owner: item(IDEA) })
        .expect("query external references");
    assert!(
        matches!(references, LinkCommandResult::References(references)
        if references.contains(&path) && references.contains(&url))
    );

    let service_references = host
        .services
        .borrow_mut()
        .call(ReferencesForItemV1::new(item(IDEA)))
        .expect("query references service");
    assert_eq!(service_references.len(), 2);
}

#[test]
fn directed_link_removal_preserves_reverse_and_unrelated_relations() {
    let file = TestDatabase::new();
    let mut host = link_host(file.open());
    host.commands
        .execute(LinkCommand::Add {
            source: item(IDEA),
            target: item(NOTE),
        })
        .unwrap();
    host.commands
        .execute(LinkCommand::Add {
            source: item(PROJECT),
            target: item(IDEA),
        })
        .unwrap();

    host.commands
        .execute(LinkCommand::Remove {
            source: item(IDEA),
            target: item(NOTE),
        })
        .expect("remove directed link");

    assert!(matches!(
        host.commands
            .execute(LinkCommand::Outgoing { item: item(IDEA) })
            .unwrap(),
        LinkCommandResult::Items(items) if items.is_empty()
    ));
    assert!(matches!(
        host.commands
            .execute(LinkCommand::Backlinks { item: item(IDEA) })
            .unwrap(),
        LinkCommandResult::Items(items)
            if items.len() == 1 && items[0].reference() == &item(PROJECT)
    ));
}

#[test]
fn external_reference_removal_is_persistent_and_reports_missing_entries() {
    let file = TestDatabase::new();
    let mut host = link_host(file.open());
    let reference = ExternalReference::file("research/meeting notes.md").unwrap();
    host.commands
        .execute(LinkCommand::AddReference {
            owner: item(NOTE),
            reference: reference.clone(),
        })
        .expect("add external reference");

    host.commands
        .execute(LinkCommand::RemoveReference {
            owner: item(NOTE),
            reference: reference.clone(),
        })
        .expect("remove external reference");
    assert!(matches!(
        host.commands
            .execute(LinkCommand::References { owner: item(NOTE) })
            .unwrap(),
        LinkCommandResult::References(references) if references.is_empty()
    ));
    assert!(
        host.commands
            .execute(LinkCommand::RemoveReference {
                owner: item(NOTE),
                reference,
            })
            .is_err()
    );
}

#[test]
fn links_reject_missing_items_and_self_relations() {
    let file = TestDatabase::new();
    let mut host = link_host(file.open());
    let missing = ItemRef::new("note", MISSING_ID).unwrap();

    assert!(matches!(
        host.commands.execute(LinkCommand::Add {
            source: item(IDEA),
            target: missing,
        }),
        Err(error) if error.to_string().contains("was not found")
    ));
    assert!(matches!(
        host.commands.execute(LinkCommand::Add {
            source: item(IDEA),
            target: item(IDEA),
        }),
        Err(error) if error.to_string().contains("cannot link to itself")
    ));
}

#[test]
fn item_removal_cleans_outgoing_incoming_links_and_owned_references() {
    let file = TestDatabase::new();
    let mut host = link_host(file.open());
    let changed = Rc::new(RefCell::new(false));
    let event_target = Rc::clone(&changed);
    let _subscription = host.events.subscribe(move |_: &LinksChangedV1| {
        *event_target.borrow_mut() = true;
    });
    host.commands
        .execute(LinkCommand::Add {
            source: item(IDEA),
            target: item(NOTE),
        })
        .unwrap();
    host.commands
        .execute(LinkCommand::Add {
            source: item(PROJECT),
            target: item(IDEA),
        })
        .unwrap();
    host.commands
        .execute(LinkCommand::AddReference {
            owner: item(IDEA),
            reference: ExternalReference::file("research.md").unwrap(),
        })
        .unwrap();

    *changed.borrow_mut() = false;
    host.events.publish(ItemRemovedV1::new(item(IDEA)));

    assert!(matches!(
        host.commands
            .execute(LinkCommand::Outgoing { item: item(IDEA) })
            .unwrap(),
        LinkCommandResult::Items(items) if items.is_empty()
    ));
    assert!(matches!(
        host.commands
            .execute(LinkCommand::Backlinks { item: item(NOTE) })
            .unwrap(),
        LinkCommandResult::Items(items) if items.is_empty()
    ));
    assert!(matches!(
        host.commands
            .execute(LinkCommand::Outgoing { item: item(PROJECT) })
            .unwrap(),
        LinkCommandResult::Items(items) if items.is_empty()
    ));
    assert!(matches!(
        host.commands
            .execute(LinkCommand::Backlinks { item: item(IDEA) })
            .unwrap(),
        LinkCommandResult::Items(items) if items.is_empty()
    ));
    assert!(matches!(
        host.commands
            .execute(LinkCommand::References { owner: item(IDEA) })
            .unwrap(),
        LinkCommandResult::References(references) if references.is_empty()
    ));
    assert!(*changed.borrow());
}

#[test]
fn links_view_can_select_an_item_and_add_a_link_through_the_host() {
    let file = TestDatabase::new();
    let host = link_host(file.open());
    let commands = Rc::new(RefCell::new(host.commands));
    let mut workspace = Workspace::new();
    workspace
        .register_view(LinksView::new(commands, &host.events))
        .expect("register Links view");

    workspace.handle_key(key(KeyCode::Char('s')));
    for character in IDEA.chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    workspace.handle_key(key(KeyCode::Char('a')));
    for character in NOTE.chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    let rendered = render_text(&mut workspace);

    assert!(rendered.contains("Research note"));
    assert!(rendered.contains(NOTE));
}

#[test]
fn links_view_leaves_j_and_k_for_workspace_navigation() {
    let events = EventBus::new();
    let commands = Rc::new(RefCell::new(CommandDispatcher::new()));
    let mut view = LinksView::new(commands, &events);

    assert!(!view.handle_key(key(KeyCode::Char('j')), FocusArea::Content));
    assert!(!view.handle_key(key(KeyCode::Char('k')), FocusArea::Content));
}

#[test]
fn newer_link_schema_versions_are_rejected() {
    let file = TestDatabase::new();
    let database = file.open();
    database
        .with_connection(|connection| {
            connection.execute_batch(
                "CREATE TABLE tuilane_links_migrations (version INTEGER PRIMARY KEY NOT NULL);",
            )?;
            connection.execute(
                "INSERT INTO tuilane_links_migrations (version) VALUES (?1)",
                [99],
            )?;
            Ok(())
        })
        .expect("prepare newer schema version");

    assert!(matches!(
        LinksPlugin::new(database),
        Err(LinkError::UnsupportedSchemaVersion(99))
    ));
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_text(workspace: &mut Workspace) -> String {
    let mut terminal = Terminal::new(TestBackend::new(140, 36)).expect("create test terminal");
    terminal
        .draw(|frame| workspace.render(frame))
        .expect("render Links workspace");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
