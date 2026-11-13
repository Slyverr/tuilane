//! First-party Tasks feature: validated commands, durable SQLite records, and a TUI view.

mod view;

pub use view::TasksView;

use std::{error::Error, fmt};

use rusqlite::{OptionalExtension, Row, params, types::Type};
use tuilane_core::{
    CommandDispatcher, CommandRegistrationError, EventBus, ServiceBroker, ServiceRegistrationError,
};
use tuilane_plugin_api::items::{
    ItemRef, ItemReferenceError, ItemRemovedV1, ItemSummary, ItemSummaryError,
};
use tuilane_plugin_api::tasks::{
    Task, TaskByIdV1, TaskCommand, TaskCommandResult, TaskDate, TaskId, TaskStatus, TaskSummary,
    TaskTitle, TasksByDateV1, TasksChangedV1,
};
use tuilane_storage::{SqliteDatabase, StorageError};
use uuid::Uuid;

const TASK_MIGRATIONS: &str =
    "CREATE TABLE IF NOT EXISTS tuilane_tasks_migrations (version INTEGER PRIMARY KEY NOT NULL);";
const CURRENT_SCHEMA_VERSION: i64 = 1;
const TASK_COLUMNS: &str = "id, title, due_date, status, created_at, updated_at";

/// Errors returned by the Tasks feature.
#[derive(Debug)]
pub enum TaskError {
    Storage(StorageError),
    NotFound(TaskId),
    UnsupportedSchemaVersion(i64),
    InvalidStoredValue(String),
    ItemReference(ItemReferenceError),
    ItemSummary(ItemSummaryError),
}

impl fmt::Display for TaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::NotFound(id) => write!(formatter, "task '{id}' was not found"),
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "task database schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
            Self::InvalidStoredValue(value) => {
                write!(
                    formatter,
                    "task database contains an invalid value: {value}"
                )
            }
            Self::ItemReference(error) => error.fmt(formatter),
            Self::ItemSummary(error) => error.fmt(formatter),
        }
    }
}

impl Error for TaskError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::ItemReference(error) => Some(error),
            Self::ItemSummary(error) => Some(error),
            Self::NotFound(_) | Self::UnsupportedSchemaVersion(_) | Self::InvalidStoredValue(_) => {
                None
            }
        }
    }
}

impl From<StorageError> for TaskError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<ItemReferenceError> for TaskError {
    fn from(error: ItemReferenceError) -> Self {
        Self::ItemReference(error)
    }
}

impl From<ItemSummaryError> for TaskError {
    fn from(error: ItemSummaryError) -> Self {
        Self::ItemSummary(error)
    }
}

/// First-party Tasks feature backed by the host's SQLite database.
#[derive(Clone)]
pub struct TasksPlugin {
    database: SqliteDatabase,
}

/// Errors returned while registering Tasks commands and services with the host.
#[derive(Debug)]
pub enum TaskRegistrationError {
    Command(CommandRegistrationError),
    Service(ServiceRegistrationError),
}

impl fmt::Display for TaskRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => error.fmt(formatter),
            Self::Service(error) => error.fmt(formatter),
        }
    }
}

impl Error for TaskRegistrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Service(error) => Some(error),
        }
    }
}

impl TasksPlugin {
    /// Opens the task schema and applies any required migrations.
    pub fn new(database: SqliteDatabase) -> Result<Self, TaskError> {
        let plugin = Self { database };
        plugin.migrate()?;
        Ok(plugin)
    }

    /// Registers the shared task command path and versioned date-query service.
    pub fn register(
        self,
        commands: &mut CommandDispatcher,
        services: &mut ServiceBroker,
        events: EventBus,
    ) -> Result<(), TaskRegistrationError> {
        let command_provider = self.clone();
        let command_events = events.clone();
        commands
            .register(
                move |command: TaskCommand| -> Result<TaskCommandResult, TaskError> {
                    let notify =
                        !matches!(&command, TaskCommand::Get { .. } | TaskCommand::List { .. });
                    let removed_item = match &command {
                        TaskCommand::Delete { id } => Some(ItemRef::new("task", id.as_str())?),
                        _ => None,
                    };
                    let response = command_provider.execute(command)?;
                    if notify {
                        command_events.publish(TasksChangedV1);
                    }
                    if let Some(item) = removed_item {
                        command_events.publish(ItemRemovedV1::new(item));
                    }
                    Ok(response)
                },
            )
            .map_err(TaskRegistrationError::Command)?;
        let date_provider = self.clone();
        services
            .register(move |query: TasksByDateV1| date_provider.tasks_by_date(query))
            .map_err(TaskRegistrationError::Service)?;
        services
            .register(move |query: TaskByIdV1| self.item_by_id(query.id()))
            .map_err(TaskRegistrationError::Service)?;
        Ok(())
    }

    fn item_by_id(&self, id: &TaskId) -> Result<Option<ItemSummary>, TaskError> {
        let task = match self.get(id) {
            Ok(task) => task,
            Err(TaskError::NotFound(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let reference = ItemRef::new("task", task.id().as_str())?;
        let summary = ItemSummary::new(reference, task.title().as_str())?;
        Ok(Some(summary))
    }

    /// Executes one validated task operation.
    fn execute(&self, command: TaskCommand) -> Result<TaskCommandResult, TaskError> {
        match command {
            TaskCommand::Create { title, due_date } => {
                self.create(title, due_date).map(TaskCommandResult::Task)
            }
            TaskCommand::Get { id } => self.get(&id).map(TaskCommandResult::Task),
            TaskCommand::List { include_completed } => {
                self.list(include_completed).map(TaskCommandResult::Tasks)
            }
            TaskCommand::Rename { id, title } => {
                self.rename(&id, &title).map(TaskCommandResult::Task)
            }
            TaskCommand::SetDueDate { id, due_date } => self
                .set_due_date(&id, due_date.as_ref())
                .map(TaskCommandResult::Task),
            TaskCommand::Complete { id } => self
                .set_status(&id, TaskStatus::Completed)
                .map(TaskCommandResult::Task),
            TaskCommand::Reopen { id } => self
                .set_status(&id, TaskStatus::Open)
                .map(TaskCommandResult::Task),
            TaskCommand::Delete { id } => self.delete(&id).map(TaskCommandResult::Deleted),
        }
    }

    fn migrate(&self) -> Result<(), TaskError> {
        self.database
            .with_connection(|connection| connection.execute_batch(TASK_MIGRATIONS))?;
        let current = self.database.with_connection(|connection| {
            connection.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM tuilane_tasks_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
        })?;
        if current > CURRENT_SCHEMA_VERSION {
            return Err(TaskError::UnsupportedSchemaVersion(current));
        }
        if current == 0 {
            self.database.transaction(|transaction| {
                transaction.execute_batch(include_str!("../migrations/0001_tasks.sql"))?;
                transaction.execute(
                    "INSERT INTO tuilane_tasks_migrations (version) VALUES (?1)",
                    [CURRENT_SCHEMA_VERSION],
                )?;
                Ok(())
            })?;
        }
        Ok(())
    }

    fn create(&self, title: TaskTitle, due_date: Option<TaskDate>) -> Result<Task, TaskError> {
        let id = TaskId::new(Uuid::new_v4().to_string()).expect("UUID has canonical format");
        self.database
            .transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO tasks (id, title, due_date) VALUES (?1, ?2, ?3)",
                    params![
                        id.as_str(),
                        title.as_str(),
                        due_date.as_ref().map(TaskDate::as_str)
                    ],
                )?;
                transaction.query_row(
                    &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
                    [id.as_str()],
                    read_task,
                )
            })
            .map_err(TaskError::Storage)
    }

    fn get(&self, id: &TaskId) -> Result<Task, TaskError> {
        self.database
            .with_connection(|connection| {
                connection
                    .query_row(
                        &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
                        [id.as_str()],
                        read_task,
                    )
                    .optional()
            })?
            .ok_or_else(|| TaskError::NotFound(id.clone()))
    }

    fn list(&self, include_completed: bool) -> Result<Vec<Task>, TaskError> {
        self.database
            .with_connection(|connection| {
                let sql = if include_completed {
                    format!("SELECT {TASK_COLUMNS} FROM tasks ORDER BY due_date IS NULL, due_date, created_at, id")
                } else {
                    format!("SELECT {TASK_COLUMNS} FROM tasks WHERE status = 'open' ORDER BY due_date IS NULL, due_date, created_at, id")
                };
                let mut statement = connection.prepare(&sql)?;
                statement
                    .query_map([], read_task)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(TaskError::Storage)
    }

    fn rename(&self, id: &TaskId, title: &TaskTitle) -> Result<Task, TaskError> {
        self.update_task(id, |transaction| {
            transaction.execute(
                "UPDATE tasks SET title = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![title.as_str(), id.as_str()],
            )
        })
    }

    fn set_due_date(&self, id: &TaskId, due_date: Option<&TaskDate>) -> Result<Task, TaskError> {
        self.update_task(id, |transaction| {
            transaction.execute(
                "UPDATE tasks SET due_date = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![due_date.map(TaskDate::as_str), id.as_str()],
            )
        })
    }

    fn set_status(&self, id: &TaskId, status: TaskStatus) -> Result<Task, TaskError> {
        self.update_task(id, |transaction| {
            transaction.execute(
                "UPDATE tasks SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![status.as_str(), id.as_str()],
            )
        })
    }

    fn update_task(
        &self,
        id: &TaskId,
        update: impl FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<usize>,
    ) -> Result<Task, TaskError> {
        let task = self.database.transaction(|transaction| {
            if update(transaction)? == 0 {
                return Ok(None);
            }
            transaction
                .query_row(
                    &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
                    [id.as_str()],
                    read_task,
                )
                .map(Some)
        })?;
        task.ok_or_else(|| TaskError::NotFound(id.clone()))
    }

    fn delete(&self, id: &TaskId) -> Result<TaskId, TaskError> {
        let deleted = self.database.transaction(|transaction| {
            transaction.execute("DELETE FROM tasks WHERE id = ?1", [id.as_str()])
        })?;
        if deleted == 0 {
            return Err(TaskError::NotFound(id.clone()));
        }
        Ok(id.clone())
    }

    fn tasks_by_date(&self, query: TasksByDateV1) -> Result<Vec<TaskSummary>, TaskError> {
        let tasks = self.database.with_connection(|connection| {
            let sql = format!(
                "SELECT {TASK_COLUMNS} FROM tasks WHERE due_date = ?1 ORDER BY status, created_at, id"
            );
            let mut statement = connection.prepare(&sql)?;
            statement
                .query_map([query.date().as_str()], read_task)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })?;
        Ok(tasks.iter().map(TaskSummary::from_task).collect())
    }
}

fn read_task(row: &Row<'_>) -> rusqlite::Result<Task> {
    let id = row.get::<_, String>(0)?;
    let title = row.get::<_, String>(1)?;
    let due_date = row.get::<_, Option<String>>(2)?;
    let status = row.get::<_, String>(3)?;
    let created_at = row.get::<_, String>(4)?;
    let updated_at = row.get::<_, String>(5)?;

    let id = TaskId::new(id).map_err(|error| conversion_error(0, error))?;
    let title = TaskTitle::new(title).map_err(|error| conversion_error(1, error))?;
    let due_date = due_date
        .map(|value| TaskDate::parse(value).map_err(|error| conversion_error(2, error)))
        .transpose()?;
    let status = match status.as_str() {
        "open" => TaskStatus::Open,
        "completed" => TaskStatus::Completed,
        _ => return Err(conversion_error(3, TaskError::InvalidStoredValue(status))),
    };

    Ok(Task::new(
        id, title, due_date, status, created_at, updated_at,
    ))
}

fn conversion_error(index: usize, error: impl Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}
