//! First-party Ideas feature with validated commands and durable SQLite records.

mod view;

pub use view::IdeasView;

use std::{error::Error, fmt};

use rusqlite::{OptionalExtension, Row, params, types::Type};
use tuilane_core::{
    CommandDispatcher, CommandRegistrationError, EventBus, ServiceBroker, ServiceRegistrationError,
};
use tuilane_plugin_api::ideas::{
    Idea, IdeaByIdV1, IdeaCommand, IdeaCommandResult, IdeaId, IdeaIdError, IdeaTitle,
    IdeaTitleError, IdeasChangedV1,
};
use tuilane_plugin_api::items::{ItemRef, ItemReferenceError, ItemRemovedV1, ItemSummary};
use tuilane_storage::{SqliteDatabase, StorageError};
use uuid::Uuid;

const IDEA_MIGRATIONS: &str =
    "CREATE TABLE IF NOT EXISTS tuilane_ideas_migrations (version INTEGER PRIMARY KEY NOT NULL);";
const CURRENT_SCHEMA_VERSION: i64 = 1;
const IDEA_COLUMNS: &str = "id, title, created_at, updated_at";

/// Errors returned by the Ideas feature.
#[derive(Debug)]
pub enum IdeaError {
    Storage(StorageError),
    NotFound(IdeaId),
    UnsupportedSchemaVersion(i64),
    InvalidStoredValue(String),
    InvalidId(IdeaIdError),
    InvalidTitle(IdeaTitleError),
    InvalidItemReference(ItemReferenceError),
}

impl fmt::Display for IdeaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::NotFound(id) => write!(formatter, "idea '{id}' was not found"),
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "ideas database schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
            Self::InvalidStoredValue(value) => {
                write!(
                    formatter,
                    "ideas database contains an invalid value: {value}"
                )
            }
            Self::InvalidId(error) => error.fmt(formatter),
            Self::InvalidTitle(error) => error.fmt(formatter),
            Self::InvalidItemReference(error) => error.fmt(formatter),
        }
    }
}

impl Error for IdeaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::InvalidId(error) => Some(error),
            Self::InvalidTitle(error) => Some(error),
            Self::InvalidItemReference(error) => Some(error),
            Self::NotFound(_) | Self::UnsupportedSchemaVersion(_) | Self::InvalidStoredValue(_) => {
                None
            }
        }
    }
}

impl From<StorageError> for IdeaError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<IdeaIdError> for IdeaError {
    fn from(error: IdeaIdError) -> Self {
        Self::InvalidId(error)
    }
}

impl From<IdeaTitleError> for IdeaError {
    fn from(error: IdeaTitleError) -> Self {
        Self::InvalidTitle(error)
    }
}

impl From<ItemReferenceError> for IdeaError {
    fn from(error: ItemReferenceError) -> Self {
        Self::InvalidItemReference(error)
    }
}

/// Errors returned while registering Ideas with the host.
#[derive(Debug)]
pub enum IdeaRegistrationError {
    Command(CommandRegistrationError),
    Service(ServiceRegistrationError),
}

impl fmt::Display for IdeaRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => error.fmt(formatter),
            Self::Service(error) => error.fmt(formatter),
        }
    }
}

impl Error for IdeaRegistrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Service(error) => Some(error),
        }
    }
}

/// First-party Ideas feature backed by the host's SQLite database.
#[derive(Clone)]
pub struct IdeasPlugin {
    database: SqliteDatabase,
}

impl IdeasPlugin {
    /// Opens the idea schema and applies any required migrations.
    pub fn new(database: SqliteDatabase) -> Result<Self, IdeaError> {
        let plugin = Self { database };
        plugin.migrate()?;
        Ok(plugin)
    }

    /// Registers the shared idea command path and versioned item lookup service.
    pub fn register(
        self,
        commands: &mut CommandDispatcher,
        services: &mut ServiceBroker,
        events: EventBus,
    ) -> Result<(), IdeaRegistrationError> {
        let command_provider = self.clone();
        let command_events = events.clone();
        commands
            .register(
                move |command: IdeaCommand| -> Result<IdeaCommandResult, IdeaError> {
                    let changed = !matches!(&command, IdeaCommand::Get { .. } | IdeaCommand::List);
                    let removed_item = match &command {
                        IdeaCommand::Delete { id } => Some(ItemRef::new("idea", id.as_str())?),
                        _ => None,
                    };
                    let response = command_provider.execute(command)?;
                    if changed {
                        command_events.publish(IdeasChangedV1);
                    }
                    if let Some(item) = removed_item {
                        command_events.publish(ItemRemovedV1::new(item));
                    }
                    Ok(response)
                },
            )
            .map_err(IdeaRegistrationError::Command)?;
        services
            .register(move |query: IdeaByIdV1| self.item_by_id(query.id()))
            .map_err(IdeaRegistrationError::Service)?;
        Ok(())
    }

    fn item_by_id(&self, id: &IdeaId) -> Result<Option<ItemSummary>, IdeaError> {
        let idea = match self.get(id) {
            Ok(idea) => idea,
            Err(IdeaError::NotFound(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let reference = ItemRef::new("idea", idea.id().as_str())?;
        let summary = ItemSummary::new(reference, idea.title().as_str())
            .map_err(|error| IdeaError::InvalidStoredValue(error.to_string()))?;
        Ok(Some(summary))
    }

    fn execute(&self, command: IdeaCommand) -> Result<IdeaCommandResult, IdeaError> {
        match command {
            IdeaCommand::Create { title } => self.create(title).map(IdeaCommandResult::Idea),
            IdeaCommand::Get { id } => self.get(&id).map(IdeaCommandResult::Idea),
            IdeaCommand::List => self.list().map(IdeaCommandResult::Ideas),
            IdeaCommand::Rename { id, title } => {
                self.rename(&id, &title).map(IdeaCommandResult::Idea)
            }
            IdeaCommand::Delete { id } => self.delete(&id).map(IdeaCommandResult::Deleted),
        }
    }

    fn migrate(&self) -> Result<(), IdeaError> {
        self.database
            .with_connection(|connection| connection.execute_batch(IDEA_MIGRATIONS))?;
        let current = self.database.with_connection(|connection| {
            connection.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM tuilane_ideas_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
        })?;
        if current > CURRENT_SCHEMA_VERSION {
            return Err(IdeaError::UnsupportedSchemaVersion(current));
        }
        if current == 0 {
            self.database.transaction(|transaction| {
                transaction.execute_batch(include_str!("../migrations/0001_ideas.sql"))?;
                transaction.execute(
                    "INSERT INTO tuilane_ideas_migrations (version) VALUES (?1)",
                    [CURRENT_SCHEMA_VERSION],
                )?;
                Ok(())
            })?;
        }
        Ok(())
    }

    fn create(&self, title: IdeaTitle) -> Result<Idea, IdeaError> {
        let id = IdeaId::new(Uuid::new_v4().to_string()).expect("UUID has canonical format");
        self.database
            .transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO ideas (id, title) VALUES (?1, ?2)",
                    params![id.as_str(), title.as_str()],
                )?;
                transaction.query_row(
                    &format!("SELECT {IDEA_COLUMNS} FROM ideas WHERE id = ?1"),
                    [id.as_str()],
                    read_idea,
                )
            })
            .map_err(IdeaError::Storage)
    }

    fn get(&self, id: &IdeaId) -> Result<Idea, IdeaError> {
        self.database
            .with_connection(|connection| {
                connection
                    .query_row(
                        &format!("SELECT {IDEA_COLUMNS} FROM ideas WHERE id = ?1"),
                        [id.as_str()],
                        read_idea,
                    )
                    .optional()
            })?
            .ok_or_else(|| IdeaError::NotFound(id.clone()))
    }

    fn list(&self) -> Result<Vec<Idea>, IdeaError> {
        self.database
            .with_connection(|connection| {
                let mut statement = connection.prepare(&format!(
                    "SELECT {IDEA_COLUMNS} FROM ideas ORDER BY title COLLATE NOCASE, id"
                ))?;
                statement
                    .query_map([], read_idea)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(IdeaError::Storage)
    }

    fn rename(&self, id: &IdeaId, title: &IdeaTitle) -> Result<Idea, IdeaError> {
        let idea = self.database.transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE ideas SET title = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![title.as_str(), id.as_str()],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            transaction
                .query_row(
                    &format!("SELECT {IDEA_COLUMNS} FROM ideas WHERE id = ?1"),
                    [id.as_str()],
                    read_idea,
                )
                .map(Some)
        })?;
        idea.ok_or_else(|| IdeaError::NotFound(id.clone()))
    }

    fn delete(&self, id: &IdeaId) -> Result<IdeaId, IdeaError> {
        let deleted = self.database.transaction(|transaction| {
            transaction.execute("DELETE FROM ideas WHERE id = ?1", [id.as_str()])
        })?;
        if deleted == 0 {
            return Err(IdeaError::NotFound(id.clone()));
        }
        Ok(id.clone())
    }
}

fn read_idea(row: &Row<'_>) -> rusqlite::Result<Idea> {
    let id = row.get::<_, String>(0)?;
    let title = row.get::<_, String>(1)?;
    let created_at = row.get::<_, String>(2)?;
    let updated_at = row.get::<_, String>(3)?;

    let id = IdeaId::new(id).map_err(|error| conversion_error(0, error))?;
    let title = IdeaTitle::new(title).map_err(|error| conversion_error(1, error))?;

    Ok(Idea::new(id, title, created_at, updated_at))
}

fn conversion_error(index: usize, error: impl Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}
