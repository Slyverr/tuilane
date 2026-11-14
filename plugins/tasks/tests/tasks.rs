use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tuilane_core::{
    CommandCallerId, CommandCancellation, CommandDispatchError, CommandDispatcher, EventBus,
    ServiceBroker,
};
use tuilane_plugin_api::items::{ItemRef, ItemRemovedV1};
use tuilane_plugin_api::search::TaskSearchDocumentsV1;
use tuilane_plugin_api::tasks::{
    Task, TaskByIdV1, TaskCommand, TaskCommandResult, TaskDate, TaskId, TaskStatus, TaskTitle,
    TasksByDateV1, TasksChangedV1,
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

struct TaskHost {
    commands: CommandDispatcher,
    services: ServiceBroker,
    events: EventBus,
}

fn task_host(database: SqliteDatabase) -> TaskHost {
    let plugin = TasksPlugin::new(database).expect("initialize task schema");
    let mut commands = CommandDispatcher::new();
    let mut services = ServiceBroker::new();
    let events = EventBus::new();
    plugin
        .register(&mut commands, &mut services, events.clone())
        .expect("register task contracts");
    TaskHost {
        commands,
        services,
        events,
    }
}

fn create_task(commands: &mut CommandDispatcher, title: &str, due_date: Option<&str>) -> Task {
    let command = TaskCommand::Create {
        title: TaskTitle::new(title).expect("valid task title"),
        due_date: due_date.map(|date| TaskDate::parse(date).expect("valid task date")),
    };
    match commands.execute(command).expect("create task") {
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
    let mut host = task_host(file.open());
    let later = create_task(&mut host.commands, "Later", Some("2026-11-20"));
    let earlier = create_task(&mut host.commands, "Earlier", Some("2026-11-14"));
    let undated = create_task(&mut host.commands, "Undated", None);

    host.commands
        .execute(TaskCommand::Complete {
            id: earlier.id().clone(),
        })
        .expect("complete task");

    let open = tasks(
        host.commands
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
        host.commands
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
fn untrusted_callers_need_separate_task_write_and_delete_grants() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let agent = CommandCallerId::new("agent.test").unwrap();
    let create = TaskCommand::Create {
        title: TaskTitle::new("Protected task").unwrap(),
        due_date: None,
    };

    assert!(matches!(
        host.commands
            .execute_as(&agent, create.clone())
            .into_result(),
        Err(CommandDispatchError::PermissionDenied { .. })
    ));
    assert!(
        tasks(
            host.commands
                .execute(TaskCommand::List {
                    include_completed: true,
                })
                .unwrap()
        )
        .is_empty()
    );

    host.commands.grant_permission(
        &agent,
        tuilane_plugin_api::CommandPermission::new("tasks.write"),
    );
    let prepared = host.commands.preview_as(&agent, create).unwrap();
    assert_eq!(prepared.preview().descriptor().id(), "tasks.create@1");
    assert!(
        tasks(
            host.commands
                .execute(TaskCommand::List {
                    include_completed: true,
                })
                .unwrap()
        )
        .is_empty()
    );
    let (creation_result, creation_receipt) = host.commands.confirm(prepared).into_parts();
    let task = match creation_result.unwrap() {
        TaskCommandResult::Task(task) => task,
        _ => panic!("create returns a task"),
    };
    let creation_receipt = creation_receipt.expect("authorized task creation includes a receipt");
    assert_eq!(creation_receipt.caller(), &agent);
    assert_eq!(creation_receipt.command_id(), "tasks.create@1");
    assert_eq!(
        creation_receipt.outcome(),
        tuilane_core::CommandOutcome::Succeeded
    );

    host.commands.grant_permission(
        &agent,
        tuilane_plugin_api::CommandPermission::new("tasks.read"),
    );
    let (read_result, read_receipt) = host
        .commands
        .execute_as(
            &agent,
            TaskCommand::List {
                include_completed: true,
            },
        )
        .into_parts();
    assert_eq!(tasks(read_result.unwrap()).len(), 1);
    let read_receipt = read_receipt.expect("authorized task read includes a receipt");
    assert_eq!(read_receipt.command_id(), "tasks.list@1");
    assert_eq!(read_receipt.sequence(), 2);

    assert!(matches!(
        host.commands
            .execute_as(
                &agent,
                TaskCommand::Delete {
                    id: task.id().clone(),
                },
            )
            .into_result(),
        Err(CommandDispatchError::PermissionDenied { .. })
    ));
    assert_eq!(
        tasks(
            host.commands
                .execute(TaskCommand::List {
                    include_completed: true,
                })
                .unwrap()
        )
        .len(),
        1
    );

    host.commands.grant_permission(
        &agent,
        tuilane_plugin_api::CommandPermission::new("tasks.delete"),
    );
    let prepared = host
        .commands
        .preview_as(
            &agent,
            TaskCommand::Delete {
                id: task.id().clone(),
            },
        )
        .unwrap();
    assert_eq!(prepared.preview().descriptor().id(), "tasks.delete@1");
    assert!(matches!(
        host.commands
            .execute(TaskCommand::List {
                include_completed: true,
            })
            .unwrap(),
        TaskCommandResult::Tasks(ref current) if current.len() == 1
    ));
    let (deletion_result, deletion_receipt) = host.commands.confirm(prepared).into_parts();
    assert!(matches!(
        deletion_result.unwrap(),
        TaskCommandResult::Deleted(_)
    ));
    let deletion_receipt = deletion_receipt.expect("authorized deletion includes a receipt");
    assert_eq!(deletion_receipt.command_id(), "tasks.delete@1");
    assert_eq!(deletion_receipt.sequence(), 3);
}

#[test]
fn bulk_completion_requires_confirmation_and_completes_the_exact_task_snapshots() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let first = create_task(&mut host.commands, "First task", None);
    let second = create_task(&mut host.commands, "Second task", None);
    let snapshots = tasks(
        host.commands
            .execute(TaskCommand::List {
                include_completed: false,
            })
            .unwrap(),
    );
    let event_count = Rc::new(std::cell::Cell::new(0));
    let event_target = Rc::clone(&event_count);
    let _subscription = host.events.subscribe(move |_: &TasksChangedV1| {
        event_target.set(event_target.get() + 1);
    });
    let agent = CommandCallerId::new("agent.test").unwrap();
    let command = TaskCommand::CompleteMany {
        expected_tasks: snapshots.clone(),
    };

    assert!(matches!(
        host.commands.preview_as(&agent, command.clone()),
        Err(CommandDispatchError::PermissionDenied {
            command: "tasks.complete_many@1",
            ..
        })
    ));

    host.commands.grant_permission(
        &agent,
        tuilane_plugin_api::CommandPermission::new("tasks.write"),
    );
    let prepared = host.commands.preview_as(&agent, command).unwrap();
    assert!(prepared.preview().request_details().contains("First task"));
    assert!(prepared.preview().request_details().contains("Second task"));
    assert_eq!(
        tasks(
            host.commands
                .execute(TaskCommand::List {
                    include_completed: false,
                })
                .unwrap()
        )
        .len(),
        2,
        "preparing a batch has no side effects"
    );
    assert_eq!(event_count.get(), 0);

    let (result, receipt) = host.commands.confirm(prepared).into_parts();
    let completed = tasks(result.unwrap());
    assert_eq!(completed.len(), 2);
    assert!(
        completed
            .iter()
            .all(|task| task.status() == TaskStatus::Completed)
    );
    assert!(completed.iter().any(|task| task.id() == first.id()));
    assert!(completed.iter().any(|task| task.id() == second.id()));
    let receipt = receipt.expect("authorized bulk completion includes a receipt");
    assert_eq!(receipt.command_id(), "tasks.complete_many@1");
    assert_eq!(receipt.outcome(), tuilane_core::CommandOutcome::Succeeded);
    assert_eq!(event_count.get(), 1);

    assert!(
        tasks(
            host.commands
                .execute(TaskCommand::List {
                    include_completed: false,
                })
                .unwrap()
        )
        .is_empty()
    );
}

#[test]
fn stale_bulk_snapshot_aborts_the_entire_transaction() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let first = create_task(&mut host.commands, "First task", None);
    let second = create_task(&mut host.commands, "Second task", None);
    let snapshots = tasks(
        host.commands
            .execute(TaskCommand::List {
                include_completed: false,
            })
            .unwrap(),
    );
    let agent = CommandCallerId::new("agent.test").unwrap();
    host.commands.grant_permission(
        &agent,
        tuilane_plugin_api::CommandPermission::new("tasks.write"),
    );
    let prepared = host
        .commands
        .preview_as(
            &agent,
            TaskCommand::CompleteMany {
                expected_tasks: snapshots,
            },
        )
        .unwrap();

    host.commands
        .execute(TaskCommand::Rename {
            id: second.id().clone(),
            title: TaskTitle::new("Changed after preview").unwrap(),
        })
        .unwrap();
    let (result, receipt) = host.commands.confirm(prepared).into_parts();

    assert!(matches!(
        result,
        Err(CommandDispatchError::HandlerFailed { ref message, .. })
            if message.contains("changed or no longer exists")
    ));
    let receipt = receipt.expect("attempted batch includes a failure receipt");
    assert_eq!(receipt.outcome(), tuilane_core::CommandOutcome::Failed);

    let current = tasks(
        host.commands
            .execute(TaskCommand::List {
                include_completed: true,
            })
            .unwrap(),
    );
    assert!(current.iter().all(|task| task.status() == TaskStatus::Open));
    assert!(current.iter().any(|task| task.id() == first.id()));
    assert!(current.iter().any(|task| task.id() == second.id()));
}

#[test]
fn cancelled_bulk_confirmation_does_not_change_tasks() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    create_task(&mut host.commands, "First task", None);
    create_task(&mut host.commands, "Second task", None);
    let snapshots = tasks(
        host.commands
            .execute(TaskCommand::List {
                include_completed: false,
            })
            .unwrap(),
    );
    let agent = CommandCallerId::new("agent.test").unwrap();
    host.commands.grant_permission(
        &agent,
        tuilane_plugin_api::CommandPermission::new("tasks.write"),
    );
    let prepared = host
        .commands
        .preview_as(
            &agent,
            TaskCommand::CompleteMany {
                expected_tasks: snapshots,
            },
        )
        .unwrap();
    let cancellation = CommandCancellation::new();
    assert!(cancellation.cancel());

    let (result, receipt) = host
        .commands
        .confirm_with_cancellation(prepared, &cancellation)
        .into_parts();

    assert_eq!(
        result,
        Err(CommandDispatchError::Cancelled {
            command: "tasks.complete_many@1"
        })
    );
    assert!(receipt.is_none());
    let unchanged = tasks(
        host.commands
            .execute(TaskCommand::List {
                include_completed: false,
            })
            .unwrap(),
    );
    assert_eq!(unchanged.len(), 2);
    assert!(
        unchanged
            .iter()
            .all(|task| task.status() == TaskStatus::Open)
    );
}

#[test]
fn bulk_completion_rejects_empty_oversized_and_duplicate_batches() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let task = create_task(&mut host.commands, "One task", None);
    let snapshot = tasks(
        host.commands
            .execute(TaskCommand::List {
                include_completed: false,
            })
            .unwrap(),
    )
    .remove(0);

    for expected_tasks in [
        Vec::new(),
        vec![snapshot.clone(), snapshot.clone()],
        vec![snapshot.clone(); tuilane_plugin_api::tasks::MAX_TASK_BATCH_SIZE + 1],
    ] {
        assert!(matches!(
            host.commands
                .execute(TaskCommand::CompleteMany { expected_tasks }),
            Err(CommandDispatchError::HandlerFailed { .. })
        ));
    }

    let unchanged = match host
        .commands
        .execute(TaskCommand::Get {
            id: task.id().clone(),
        })
        .unwrap()
    {
        TaskCommandResult::Task(task) => task,
        _ => panic!("get returns one task"),
    };
    assert_eq!(unchanged.status(), TaskStatus::Open);
}

#[test]
fn task_updates_and_deletes_report_missing_records() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let task = create_task(&mut host.commands, "Draft", None);
    let id = task.id().clone();

    let renamed = host
        .commands
        .execute(TaskCommand::Rename {
            id: id.clone(),
            title: TaskTitle::new("Review").expect("valid title"),
        })
        .expect("rename task");
    assert!(
        matches!(renamed, TaskCommandResult::Task(ref task) if task.title().as_str() == "Review")
    );

    let dated = host
        .commands
        .execute(TaskCommand::SetDueDate {
            id: id.clone(),
            due_date: Some(TaskDate::parse("2026-11-20").expect("valid date")),
        })
        .expect("set due date");
    assert!(
        matches!(dated, TaskCommandResult::Task(ref task) if task.due_date().map(TaskDate::as_str) == Some("2026-11-20"))
    );

    host.commands
        .execute(TaskCommand::SetDueDate {
            id: id.clone(),
            due_date: None,
        })
        .expect("clear due date");
    host.commands
        .execute(TaskCommand::Complete { id: id.clone() })
        .expect("complete task");
    let reopened = host
        .commands
        .execute(TaskCommand::Reopen { id: id.clone() })
        .expect("reopen task");
    assert!(
        matches!(reopened, TaskCommandResult::Task(ref task) if task.status() == TaskStatus::Open)
    );

    assert_eq!(
        host.commands
            .execute(TaskCommand::Delete { id: id.clone() })
            .expect("delete task"),
        TaskCommandResult::Deleted(id.clone())
    );
    let missing = host
        .commands
        .execute(TaskCommand::Get { id: id.clone() })
        .expect_err("deleted task cannot be read");
    assert!(missing.to_string().contains(&id.to_string()));
    assert!(missing.to_string().contains("was not found"));
}

#[test]
fn tasks_survive_reopening_the_database() {
    let file = TestDatabase::new();
    let task_id = {
        let mut host = task_host(file.open());
        create_task(&mut host.commands, "Persisted", Some("2026-11-20"))
            .id()
            .clone()
    };

    let mut host = task_host(file.open());
    let result = host
        .commands
        .execute(TaskCommand::Get { id: task_id })
        .expect("read persisted task");
    assert!(
        matches!(result, TaskCommandResult::Task(task) if task.title().as_str() == "Persisted")
    );
}

#[test]
fn date_query_is_discovered_and_routed_through_the_host() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());

    let command_result = host
        .commands
        .execute(TaskCommand::Create {
            title: TaskTitle::new("Calendar item").expect("valid title"),
            due_date: Some(TaskDate::parse("2026-11-20").expect("valid date")),
        })
        .expect("create through command dispatcher");
    assert!(matches!(command_result, TaskCommandResult::Task(_)));

    let query = TasksByDateV1::new(TaskDate::parse("2026-11-20").expect("valid date"));
    let results = host
        .services
        .call(query)
        .expect("query through service broker");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title().as_str(), "Calendar item");
}

#[test]
fn task_item_lookup_returns_public_reference_metadata() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let task = create_task(&mut host.commands, "Linkable task", None);

    let summary = host
        .services
        .call(TaskByIdV1::new(task.id().clone()))
        .expect("query task item service")
        .expect("task exists");

    assert_eq!(
        summary.reference(),
        &ItemRef::new("task", task.id().as_str()).expect("valid task reference")
    );
    assert_eq!(summary.title(), "Linkable task");
}

#[test]
fn task_search_service_exposes_titles_and_due_status_metadata() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let task = create_task(&mut host.commands, "Review research", Some("2026-12-01"));

    let documents = host
        .services
        .call(TaskSearchDocumentsV1)
        .expect("query task search documents");

    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].item().title(), "Review research");
    assert_eq!(
        documents[0].item().reference(),
        &ItemRef::new("task", task.id().as_str()).expect("valid task reference")
    );
    assert!(documents[0].content().contains("open"));
    assert!(documents[0].content().contains("2026-12-01"));
}

#[test]
fn task_item_lookup_returns_none_for_missing_tasks() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let id = TaskId::new("00000000-0000-4000-8000-000000000001").expect("valid task id");

    let summary = host
        .services
        .call(TaskByIdV1::new(id))
        .expect("missing task is a valid lookup");

    assert!(summary.is_none());
}

#[test]
fn task_mutations_publish_events_only_after_successful_commands() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let event_count = Rc::new(std::cell::Cell::new(0));
    let event_target = Rc::clone(&event_count);
    let _subscription = host.events.subscribe(move |_: &TasksChangedV1| {
        event_target.set(event_target.get() + 1);
    });

    let created = host
        .commands
        .execute(TaskCommand::Create {
            title: TaskTitle::new("Notify subscribers").expect("valid title"),
            due_date: None,
        })
        .expect("create task");
    let TaskCommandResult::Task(task) = created else {
        panic!("create returns a task");
    };
    assert_eq!(event_count.get(), 1);

    host.commands
        .execute(TaskCommand::List {
            include_completed: true,
        })
        .expect("list tasks");
    assert_eq!(event_count.get(), 1);

    host.commands
        .execute(TaskCommand::Delete {
            id: task.id().clone(),
        })
        .expect("delete task");
    assert_eq!(event_count.get(), 2);

    assert!(
        host.commands
            .execute(TaskCommand::Get {
                id: task.id().clone()
            })
            .is_err()
    );
    assert_eq!(event_count.get(), 2);
}

#[test]
fn task_deletion_publishes_a_stable_item_removal_event() {
    let file = TestDatabase::new();
    let mut host = task_host(file.open());
    let removed = Rc::new(RefCell::new(None));
    let event_target = Rc::clone(&removed);
    let _subscription = host.events.subscribe(move |event: &ItemRemovedV1| {
        *event_target.borrow_mut() = Some(event.item().clone());
    });
    let task = create_task(&mut host.commands, "Remove me", None);

    host.commands
        .execute(TaskCommand::Delete {
            id: task.id().clone(),
        })
        .expect("delete task");

    assert_eq!(
        *removed.borrow(),
        Some(ItemRef::new("task", task.id().as_str()).expect("valid task reference"))
    );
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
    let events = EventBus::new();
    plugin
        .register(&mut commands, &mut services, events)
        .expect("register task contracts");
    let commands = Rc::new(RefCell::new(commands));
    let mut workspace = Workspace::new();
    workspace
        .register_view(TasksView::new(commands.clone()))
        .expect("register task view");

    activate_palette_action(&mut workspace, "Create task");
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
    assert!(render_text(&mut workspace).contains("◷ 2026-11-20"));

    workspace.handle_key(key(KeyCode::Char('x')));
    assert!(render_text(&mut workspace).contains("✓  write q"));

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
