use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_core::{CommandDispatcher, EventBus, ServiceBroker};
use tuilane_plugin_api::items::{ItemRef, ItemRemovedV1};
use tuilane_plugin_api::projects::{
    ProjectByIdV1, ProjectCommand, ProjectCommandResult, ProjectId, ProjectTitle,
};
use tuilane_plugin_api::search::ProjectSearchDocumentsV1;
use tuilane_projects::{ProjectError, ProjectsPlugin, ProjectsView};
use tuilane_storage::SqliteDatabase;
use tuilane_ui::Workspace;
use uuid::Uuid;

struct TestDatabase {
    directory: PathBuf,
}

impl TestDatabase {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("tuilane-projects-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("create temporary database directory");
        Self { directory }
    }

    fn open(&self) -> SqliteDatabase {
        SqliteDatabase::open(self.directory.join("projects.sqlite3"))
            .expect("open temporary database")
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).expect("remove temporary database directory");
    }
}

struct ProjectHost {
    commands: CommandDispatcher,
    services: ServiceBroker,
    events: EventBus,
}

fn project_host(database: SqliteDatabase) -> ProjectHost {
    let plugin = ProjectsPlugin::new(database).expect("initialize project schema");
    let mut commands = CommandDispatcher::new();
    let mut services = ServiceBroker::new();
    let events = EventBus::new();
    plugin
        .register(&mut commands, &mut services, events.clone())
        .expect("register project contracts");
    ProjectHost {
        commands,
        services,
        events,
    }
}

fn create_project(
    commands: &mut CommandDispatcher,
    title: &str,
) -> tuilane_plugin_api::projects::Project {
    match commands
        .execute(ProjectCommand::Create {
            title: ProjectTitle::new(title).expect("valid project title"),
        })
        .expect("create project")
    {
        ProjectCommandResult::Project(project) => project,
        _ => panic!("create returns a project"),
    }
}

#[test]
fn project_changes_use_host_commands_and_survive_reopening_the_database() {
    let file = TestDatabase::new();
    let id = {
        let mut host = project_host(file.open());
        let project = create_project(&mut host.commands, "Roadmap");
        host.commands
            .execute(ProjectCommand::Rename {
                id: project.id().clone(),
                title: ProjectTitle::new("Product roadmap").expect("valid project title"),
            })
            .expect("rename project");
        project.id().clone()
    };

    let mut host = project_host(file.open());
    let result = host
        .commands
        .execute(ProjectCommand::Get { id })
        .expect("read persisted project");

    assert!(
        matches!(result, ProjectCommandResult::Project(ref project) if project.title().as_str() == "Product roadmap")
    );
}

#[test]
fn project_item_lookup_returns_stable_metadata() {
    let file = TestDatabase::new();
    let mut host = project_host(file.open());
    let project = create_project(&mut host.commands, "Connected work");

    let summary = host
        .services
        .call(ProjectByIdV1::new(project.id().clone()))
        .expect("query project item service")
        .expect("project exists");

    assert_eq!(
        summary.reference(),
        &ItemRef::new("project", project.id().as_str()).expect("valid item reference")
    );
    assert_eq!(summary.title(), "Connected work");
}

#[test]
fn project_search_service_exposes_project_titles() {
    let file = TestDatabase::new();
    let mut host = project_host(file.open());
    let project = create_project(&mut host.commands, "Research workspace");

    let documents = host
        .services
        .call(ProjectSearchDocumentsV1)
        .expect("query project search documents");

    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].item().title(), "Research workspace");
    assert_eq!(
        documents[0].item().reference(),
        &ItemRef::new("project", project.id().as_str()).expect("valid project reference")
    );
}

#[test]
fn successful_project_deletion_publishes_a_stable_removal_event() {
    let file = TestDatabase::new();
    let mut host = project_host(file.open());
    let removed = Rc::new(RefCell::new(None));
    let event_target = Rc::clone(&removed);
    let _subscription = host.events.subscribe(move |event: &ItemRemovedV1| {
        *event_target.borrow_mut() = Some(event.item().clone());
    });
    let project = create_project(&mut host.commands, "Remove me");

    host.commands
        .execute(ProjectCommand::Delete {
            id: project.id().clone(),
        })
        .expect("delete project");

    assert_eq!(
        *removed.borrow(),
        Some(ItemRef::new("project", project.id().as_str()).expect("valid item reference"))
    );
}

#[test]
fn missing_projects_return_a_domain_error() {
    let file = TestDatabase::new();
    let mut host = project_host(file.open());
    let id = ProjectId::new("00000000-0000-4000-8000-000000000001").expect("valid project id");

    let error = host
        .commands
        .execute(ProjectCommand::Get { id })
        .expect_err("unknown project is rejected");

    assert!(error.to_string().contains("was not found"));
}

#[test]
fn projects_view_creates_records_through_the_host_command_path() {
    let file = TestDatabase::new();
    let host = project_host(file.open());
    let commands = Rc::new(RefCell::new(host.commands));
    let mut workspace = Workspace::new();
    workspace
        .register_view(ProjectsView::new(commands, &host.events))
        .expect("register Projects view");

    workspace.handle_key(key(KeyCode::Char('c')));
    for character in "Release plan".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    let text = render_text(&mut workspace);

    assert!(text.contains("Release plan"));
}

#[test]
fn newer_project_schema_versions_are_rejected() {
    let file = TestDatabase::new();
    let database = file.open();
    database
        .with_connection(|connection| {
            connection.execute_batch(
                "CREATE TABLE tuilane_projects_migrations (version INTEGER PRIMARY KEY NOT NULL);",
            )?;
            connection.execute(
                "INSERT INTO tuilane_projects_migrations (version) VALUES (?1)",
                [99],
            )?;
            Ok(())
        })
        .expect("prepare newer schema version");

    assert!(matches!(
        ProjectsPlugin::new(database),
        Err(ProjectError::UnsupportedSchemaVersion(99))
    ));
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_text(workspace: &mut Workspace) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("create test terminal");
    terminal
        .draw(|frame| workspace.render(frame))
        .expect("render Projects workspace");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
