use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_core::{CommandDispatcher, ServiceBroker};
use tuilane_plugin_api::tasks::{
    Task, TaskCommand, TaskCommandResult, TaskDate, TaskStatus, TaskTitle, TasksByDateV1,
};
use tuilane_storage::SqliteDatabase;
use tuilane_tasks::{TaskError, TasksPlugin, TasksView};
use tuilane_ui::Workspace;
use uuid::Uuid;

struct TestDatabase {
    directory: PathBuf,
}

impl TestDatabase {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("tuilane-tasks-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("create temporary database directory");
        Self { directory }
    }

    fn open(&self) -> SqliteDatabase {
        SqliteDatabase::open(self.directory.join("tasks.sqlite3")).expect("open temporary database")
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn create_task(plugin: &TasksPlugin, title: &str, due_date: Option<&str>) -> Task {
    let command = TaskCommand::Create {
        title: TaskTitle::new(title).expect("valid task title"),
        due_date: due_date.map(|date| TaskDate::parse(date).expect("valid task date")),
    };
    match plugin.execute(command).expect("create task") {
        TaskCommandResult::Task(task) => task,
        _ => panic!("create returns a task"),
    }
}

fn tasks(result: TaskCommandResult) -> Vec<Task> {
    match result {
        TaskCommandResult::Tasks(tasks) => tasks,
        _ => panic!("list returns tasks"),
    }
}

#[test]
fn task_lists_filter_completed_records_and_sort_due_dates() {
    let file = TestDatabase::new();
    let plugin = TasksPlugin::new(file.open()).expect("initialize task schema");
    let later = create_task(&plugin, "Later", Some("2026-11-20"));
    let earlier = create_task(&plugin, "Earlier", Some("2026-11-14"));
    let undated = create_task(&plugin, "Undated", None);

    plugin
        .execute(TaskCommand::Complete {
            id: earlier.id().clone(),
        })
        .expect("complete task");

    let open = tasks(
        plugin
            .execute(TaskCommand::List {
                include_completed: false,
            })
            .expect("list open tasks"),
    );
    assert_eq!(
        open.iter()
            .map(|task| task.title().as_str())
            .collect::<Vec<_>>(),
        ["Later", "Undated"]
    );

    let all = tasks(
        plugin
            .execute(TaskCommand::List {
                include_completed: true,
            })
            .expect("list all tasks"),
    );
    assert_eq!(all[0].id(), earlier.id());
    assert_eq!(all[1].id(), later.id());
    assert_eq!(all[2].id(), undated.id());
    assert_eq!(all[0].status(), TaskStatus::Completed);
}

#[test]
fn task_updates_and_deletes_report_missing_records() {
    let file = TestDatabase::new();
    let plugin = TasksPlugin::new(file.open()).expect("initialize task schema");
    let task = create_task(&plugin, "Draft", None);
    let id = task.id().clone();

    let renamed = plugin
        .execute(TaskCommand::Rename {
            id: id.clone(),
            title: TaskTitle::new("Review").expect("valid title"),
        })
        .expect("rename task");
    assert!(
        matches!(renamed, TaskCommandResult::Task(ref task) if task.title().as_str() == "Review")
    );

    let dated = plugin
        .execute(TaskCommand::SetDueDate {
            id: id.clone(),
            due_date: Some(TaskDate::parse("2026-11-20").expect("valid date")),
        })
        .expect("set due date");
    assert!(
        matches!(dated, TaskCommandResult::Task(ref task) if task.due_date().map(TaskDate::as_str) == Some("2026-11-20"))
    );

    plugin
        .execute(TaskCommand::SetDueDate {
            id: id.clone(),
            due_date: None,
        })
        .expect("clear due date");
    plugin
        .execute(TaskCommand::Complete { id: id.clone() })
        .expect("complete task");
    let reopened = plugin
        .execute(TaskCommand::Reopen { id: id.clone() })
        .expect("reopen task");
    assert!(
        matches!(reopened, TaskCommandResult::Task(ref task) if task.status() == TaskStatus::Open)
    );

    assert_eq!(
        plugin
            .execute(TaskCommand::Delete { id: id.clone() })
            .expect("delete task"),
        TaskCommandResult::Deleted(id.clone())
    );
    assert!(matches!(
        plugin.execute(TaskCommand::Get { id: id.clone() }),
        Err(TaskError::NotFound(missing)) if missing == id
    ));
}

#[test]
fn tasks_survive_reopening_the_database() {
    let file = TestDatabase::new();
    let task_id = {
        let plugin = TasksPlugin::new(file.open()).expect("initialize task schema");
        create_task(&plugin, "Persisted", Some("2026-11-20"))
            .id()
            .clone()
    };

    let plugin = TasksPlugin::new(file.open()).expect("reopen task schema");
    let result = plugin
        .execute(TaskCommand::Get { id: task_id })
        .expect("read persisted task");
    assert!(
        matches!(result, TaskCommandResult::Task(task) if task.title().as_str() == "Persisted")
    );
}

#[test]
fn date_query_is_discovered_and_routed_through_the_host() {
    let file = TestDatabase::new();
    let plugin = TasksPlugin::new(file.open()).expect("initialize task schema");
    let mut commands = CommandDispatcher::new();
    let mut services = ServiceBroker::new();
    plugin
        .register(&mut commands, &mut services)
        .expect("register task contracts");

    let command_result = commands
        .execute(TaskCommand::Create {
            title: TaskTitle::new("Calendar item").expect("valid title"),
            due_date: Some(TaskDate::parse("2026-11-20").expect("valid date")),
        })
        .expect("create through command dispatcher");
    assert!(matches!(command_result, TaskCommandResult::Task(_)));

    let query = TasksByDateV1::new(TaskDate::parse("2026-11-20").expect("valid date"));
    let results = services.call(query).expect("query through service broker");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title().as_str(), "Calendar item");
}

#[test]
fn newer_task_schema_versions_are_rejected() {
    let file = TestDatabase::new();
    let database = file.open();
    database
        .with_connection(|connection| {
            connection.execute_batch(
                "CREATE TABLE tuilane_tasks_migrations (version INTEGER PRIMARY KEY NOT NULL);",
            )?;
            connection.execute(
                "INSERT INTO tuilane_tasks_migrations (version) VALUES (?1)",
                [99],
            )?;
            Ok(())
        })
        .expect("prepare newer schema version");

    assert!(matches!(
        TasksPlugin::new(database),
        Err(TaskError::UnsupportedSchemaVersion(99))
    ));
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_text(workspace: &mut Workspace) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("create test terminal");
    terminal
        .draw(|frame| workspace.render(frame))
        .expect("render workspace");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn task_view_routes_create_due_date_completion_and_delete_commands() {
    let file = TestDatabase::new();
    let plugin = TasksPlugin::new(file.open()).expect("initialize task schema");
    let mut commands = CommandDispatcher::new();
    let mut services = ServiceBroker::new();
    plugin
        .register(&mut commands, &mut services)
        .expect("register task contracts");
    let commands = Rc::new(RefCell::new(commands));
    let mut workspace = Workspace::new();
    workspace
        .register_view(TasksView::new(commands.clone()))
        .expect("register task view");

    workspace.handle_key(key(KeyCode::Char('c')));
    for character in "write q".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    assert!(!workspace.should_quit());
    assert!(render_text(&mut workspace).contains("write q"));

    workspace.handle_key(key(KeyCode::Char('u')));
    for character in "2026-11-20".chars() {
        workspace.handle_key(key(KeyCode::Char(character)));
    }
    workspace.handle_key(key(KeyCode::Enter));
    assert!(render_text(&mut workspace).contains("due 2026-11-20"));

    workspace.handle_key(key(KeyCode::Char('x')));
    assert!(render_text(&mut workspace).contains("[x] write q"));

    workspace.handle_key(key(KeyCode::Delete));
    workspace.handle_key(key(KeyCode::Char('y')));
    let remaining = commands
        .borrow_mut()
        .execute(TaskCommand::List {
            include_completed: true,
        })
        .expect("list after delete");
    assert_eq!(tasks(remaining).len(), 0);
}
