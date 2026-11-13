//! First-party Projects feature with validated commands and durable SQLite records.

mod view;

pub use view::ProjectsView;

use std::{error::Error, fmt};

use rusqlite::{OptionalExtension, Row, params, types::Type};
use tuilane_core::{
    CommandDispatcher, CommandRegistrationError, EventBus, ServiceBroker, ServiceRegistrationError,
};
use tuilane_plugin_api::items::{ItemRef, ItemReferenceError, ItemRemovedV1, ItemSummary};
use tuilane_plugin_api::projects::{
    Project, ProjectByIdV1, ProjectCommand, ProjectCommandResult, ProjectId, ProjectIdError,
    ProjectTitle, ProjectsChangedV1,
};
use tuilane_storage::{SqliteDatabase, StorageError};
use uuid::Uuid;

const PROJECT_MIGRATIONS: &str = "CREATE TABLE IF NOT EXISTS tuilane_projects_migrations (version INTEGER PRIMARY KEY NOT NULL);";
const CURRENT_SCHEMA_VERSION: i64 = 1;
const PROJECT_COLUMNS: &str = "id, title, created_at, updated_at";

/// Errors returned by the Projects feature.
#[derive(Debug)]
pub enum ProjectError {
    Storage(StorageError),
    NotFound(ProjectId),
    UnsupportedSchemaVersion(i64),
    InvalidStoredValue(String),
    InvalidId(ProjectIdError),
    InvalidItemReference(ItemReferenceError),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::NotFound(id) => write!(formatter, "project '{id}' was not found"),
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "project database schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
            Self::InvalidStoredValue(value) => {
                write!(
                    formatter,
                    "project database contains an invalid value: {value}"
                )
            }
            Self::InvalidId(error) => error.fmt(formatter),
            Self::InvalidItemReference(error) => error.fmt(formatter),
        }
    }
}

impl Error for ProjectError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::InvalidId(error) => Some(error),
            Self::InvalidItemReference(error) => Some(error),
            Self::NotFound(_) | Self::UnsupportedSchemaVersion(_) | Self::InvalidStoredValue(_) => {
                None
            }
        }
    }
}

impl From<StorageError> for ProjectError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<ProjectIdError> for ProjectError {
    fn from(error: ProjectIdError) -> Self {
        Self::InvalidId(error)
    }
}

impl From<ItemReferenceError> for ProjectError {
    fn from(error: ItemReferenceError) -> Self {
        Self::InvalidItemReference(error)
    }
}

/// Errors returned while registering Projects with the host.
#[derive(Debug)]
pub enum ProjectRegistrationError {
    Command(CommandRegistrationError),
    Service(ServiceRegistrationError),
}

impl fmt::Display for ProjectRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => error.fmt(formatter),
            Self::Service(error) => error.fmt(formatter),
        }
    }
}

impl Error for ProjectRegistrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Service(error) => Some(error),
        }
    }
}

/// First-party Projects feature backed by the host's SQLite database.
#[derive(Clone)]
pub struct ProjectsPlugin {
    database: SqliteDatabase,
}

impl ProjectsPlugin {
    /// Opens the project schema and applies any required migrations.
    pub fn new(database: SqliteDatabase) -> Result<Self, ProjectError> {
        let plugin = Self { database };
        plugin.migrate()?;
        Ok(plugin)
    }

    /// Registers the shared project command path and versioned item lookup service.
    pub fn register(
        self,
        commands: &mut CommandDispatcher,
        services: &mut ServiceBroker,
        events: EventBus,
    ) -> Result<(), ProjectRegistrationError> {
        let command_provider = self.clone();
        let command_events = events.clone();
        commands
            .register(
                move |command: ProjectCommand| -> Result<ProjectCommandResult, ProjectError> {
                    let changed =
                        !matches!(&command, ProjectCommand::Get { .. } | ProjectCommand::List);
                    let removed_item = match &command {
                        ProjectCommand::Delete { id } => {
                            Some(ItemRef::new("project", id.as_str())?)
                        }
                        _ => None,
                    };
                    let response = command_provider.execute(command)?;
                    if changed {
                        command_events.publish(ProjectsChangedV1);
                    }
                    if let Some(item) = removed_item {
                        command_events.publish(ItemRemovedV1::new(item));
                    }
                    Ok(response)
                },
            )
            .map_err(ProjectRegistrationError::Command)?;
        services
            .register(move |query: ProjectByIdV1| self.item_by_id(query.id()))
            .map_err(ProjectRegistrationError::Service)?;
        Ok(())
    }

    fn item_by_id(&self, id: &ProjectId) -> Result<Option<ItemSummary>, ProjectError> {
        let project = match self.get(id) {
            Ok(project) => project,
            Err(ProjectError::NotFound(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let reference = ItemRef::new("project", project.id().as_str())?;
        let summary = ItemSummary::new(reference, project.title().as_str())
            .map_err(|error| ProjectError::InvalidStoredValue(error.to_string()))?;
        Ok(Some(summary))
    }

    fn execute(&self, command: ProjectCommand) -> Result<ProjectCommandResult, ProjectError> {
        match command {
            ProjectCommand::Create { title } => {
                self.create(title).map(ProjectCommandResult::Project)
            }
            ProjectCommand::Get { id } => self.get(&id).map(ProjectCommandResult::Project),
            ProjectCommand::List => self.list().map(ProjectCommandResult::Projects),
            ProjectCommand::Rename { id, title } => {
                self.rename(&id, &title).map(ProjectCommandResult::Project)
            }
            ProjectCommand::Delete { id } => self.delete(&id).map(ProjectCommandResult::Deleted),
        }
    }

    fn migrate(&self) -> Result<(), ProjectError> {
        self.database
            .with_connection(|connection| connection.execute_batch(PROJECT_MIGRATIONS))?;
        let current = self.database.with_connection(|connection| {
            connection.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM tuilane_projects_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
        })?;
        if current > CURRENT_SCHEMA_VERSION {
            return Err(ProjectError::UnsupportedSchemaVersion(current));
        }
        if current == 0 {
            self.database.transaction(|transaction| {
                transaction.execute_batch(include_str!("../migrations/0001_projects.sql"))?;
                transaction.execute(
                    "INSERT INTO tuilane_projects_migrations (version) VALUES (?1)",
                    [CURRENT_SCHEMA_VERSION],
                )?;
                Ok(())
            })?;
        }
        Ok(())
    }

    fn create(&self, title: ProjectTitle) -> Result<Project, ProjectError> {
        let id = ProjectId::new(Uuid::new_v4().to_string()).expect("UUID has canonical format");
        self.database
            .transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO projects (id, title) VALUES (?1, ?2)",
                    params![id.as_str(), title.as_str()],
                )?;
                transaction.query_row(
                    &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"),
                    [id.as_str()],
                    read_project,
                )
            })
            .map_err(ProjectError::Storage)
    }

    fn get(&self, id: &ProjectId) -> Result<Project, ProjectError> {
        self.database
            .with_connection(|connection| {
                connection
                    .query_row(
                        &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"),
                        [id.as_str()],
                        read_project,
                    )
                    .optional()
            })?
            .ok_or_else(|| ProjectError::NotFound(id.clone()))
    }

    fn list(&self) -> Result<Vec<Project>, ProjectError> {
        self.database
            .with_connection(|connection| {
                let mut statement = connection.prepare(&format!(
                    "SELECT {PROJECT_COLUMNS} FROM projects ORDER BY title COLLATE NOCASE, id"
                ))?;
                statement
                    .query_map([], read_project)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(ProjectError::Storage)
    }

    fn rename(&self, id: &ProjectId, title: &ProjectTitle) -> Result<Project, ProjectError> {
        let project = self.database.transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE projects SET title = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![title.as_str(), id.as_str()],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            transaction
                .query_row(
                    &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"),
                    [id.as_str()],
                    read_project,
                )
                .map(Some)
        })?;
        project.ok_or_else(|| ProjectError::NotFound(id.clone()))
    }

    fn delete(&self, id: &ProjectId) -> Result<ProjectId, ProjectError> {
        let deleted = self.database.transaction(|transaction| {
            transaction.execute("DELETE FROM projects WHERE id = ?1", [id.as_str()])
        })?;
        if deleted == 0 {
            return Err(ProjectError::NotFound(id.clone()));
        }
        Ok(id.clone())
    }
}

fn read_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    let id = row.get::<_, String>(0)?;
    let title = row.get::<_, String>(1)?;
    let created_at = row.get::<_, String>(2)?;
    let updated_at = row.get::<_, String>(3)?;

    let id = ProjectId::new(id).map_err(|error| conversion_error(0, error))?;
    let title = ProjectTitle::new(title).map_err(|error| conversion_error(1, error))?;

    Ok(Project::new(id, title, created_at, updated_at))
}

fn conversion_error(index: usize, error: impl Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}
