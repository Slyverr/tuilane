//! First-party Notes feature with validated Markdown and durable SQLite records.

mod view;

pub use view::NotesView;

use std::{error::Error, fmt};

use rusqlite::{OptionalExtension, Row, params, types::Type};
use tuilane_core::{
    CommandDispatcher, CommandRegistrationError, EventBus, ServiceBroker, ServiceRegistrationError,
};
use tuilane_plugin_api::items::{ItemRef, ItemReferenceError, ItemRemovedV1, ItemSummary};
use tuilane_plugin_api::notes::{
    Markdown, MarkdownError, Note, NoteByIdV1, NoteCommand, NoteCommandResult, NoteId, NoteIdError,
    NoteTitle, NoteTitleError, NotesChangedV1,
};
use tuilane_storage::{SqliteDatabase, StorageError};
use uuid::Uuid;

const NOTE_MIGRATIONS: &str =
    "CREATE TABLE IF NOT EXISTS tuilane_notes_migrations (version INTEGER PRIMARY KEY NOT NULL);";
const CURRENT_SCHEMA_VERSION: i64 = 1;
const NOTE_COLUMNS: &str = "id, title, body, created_at, updated_at";

/// Errors returned by the Notes feature.
#[derive(Debug)]
pub enum NoteError {
    Storage(StorageError),
    NotFound(NoteId),
    UnsupportedSchemaVersion(i64),
    InvalidStoredValue(String),
    InvalidId(NoteIdError),
    InvalidTitle(NoteTitleError),
    InvalidMarkdown(MarkdownError),
    InvalidItemReference(ItemReferenceError),
}

impl fmt::Display for NoteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::NotFound(id) => write!(formatter, "note '{id}' was not found"),
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "notes database schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
            Self::InvalidStoredValue(value) => {
                write!(
                    formatter,
                    "notes database contains an invalid value: {value}"
                )
            }
            Self::InvalidId(error) => error.fmt(formatter),
            Self::InvalidTitle(error) => error.fmt(formatter),
            Self::InvalidMarkdown(error) => error.fmt(formatter),
            Self::InvalidItemReference(error) => error.fmt(formatter),
        }
    }
}

impl Error for NoteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::InvalidId(error) => Some(error),
            Self::InvalidTitle(error) => Some(error),
            Self::InvalidMarkdown(error) => Some(error),
            Self::InvalidItemReference(error) => Some(error),
            Self::NotFound(_) | Self::UnsupportedSchemaVersion(_) | Self::InvalidStoredValue(_) => {
                None
            }
        }
    }
}

impl From<StorageError> for NoteError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<NoteIdError> for NoteError {
    fn from(error: NoteIdError) -> Self {
        Self::InvalidId(error)
    }
}

impl From<NoteTitleError> for NoteError {
    fn from(error: NoteTitleError) -> Self {
        Self::InvalidTitle(error)
    }
}

impl From<MarkdownError> for NoteError {
    fn from(error: MarkdownError) -> Self {
        Self::InvalidMarkdown(error)
    }
}

impl From<ItemReferenceError> for NoteError {
    fn from(error: ItemReferenceError) -> Self {
        Self::InvalidItemReference(error)
    }
}

/// Errors returned while registering Notes with the host.
#[derive(Debug)]
pub enum NoteRegistrationError {
    Command(CommandRegistrationError),
    Service(ServiceRegistrationError),
}

impl fmt::Display for NoteRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => error.fmt(formatter),
            Self::Service(error) => error.fmt(formatter),
        }
    }
}

impl Error for NoteRegistrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Service(error) => Some(error),
        }
    }
}

/// First-party Notes feature backed by the host's SQLite database.
#[derive(Clone)]
pub struct NotesPlugin {
    database: SqliteDatabase,
}

impl NotesPlugin {
    /// Opens the note schema and applies any required migrations.
    pub fn new(database: SqliteDatabase) -> Result<Self, NoteError> {
        let plugin = Self { database };
        plugin.migrate()?;
        Ok(plugin)
    }

    /// Registers the shared note command path and versioned item lookup service.
    pub fn register(
        self,
        commands: &mut CommandDispatcher,
        services: &mut ServiceBroker,
        events: EventBus,
    ) -> Result<(), NoteRegistrationError> {
        let command_provider = self.clone();
        let command_events = events.clone();
        commands
            .register(
                move |command: NoteCommand| -> Result<NoteCommandResult, NoteError> {
                    let changed = !matches!(&command, NoteCommand::Get { .. } | NoteCommand::List);
                    let removed_item = match &command {
                        NoteCommand::Delete { id } => Some(ItemRef::new("note", id.as_str())?),
                        _ => None,
                    };
                    let response = command_provider.execute(command)?;
                    if changed {
                        command_events.publish(NotesChangedV1);
                    }
                    if let Some(item) = removed_item {
                        command_events.publish(ItemRemovedV1::new(item));
                    }
                    Ok(response)
                },
            )
            .map_err(NoteRegistrationError::Command)?;
        services
            .register(move |query: NoteByIdV1| self.item_by_id(query.id()))
            .map_err(NoteRegistrationError::Service)?;
        Ok(())
    }

    fn item_by_id(&self, id: &NoteId) -> Result<Option<ItemSummary>, NoteError> {
        let note = match self.get(id) {
            Ok(note) => note,
            Err(NoteError::NotFound(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let reference = ItemRef::new("note", note.id().as_str())?;
        let summary = ItemSummary::new(reference, note.title().as_str())
            .map_err(|error| NoteError::InvalidStoredValue(error.to_string()))?;
        Ok(Some(summary))
    }

    fn execute(&self, command: NoteCommand) -> Result<NoteCommandResult, NoteError> {
        match command {
            NoteCommand::Create { title, body } => {
                self.create(title, body).map(NoteCommandResult::Note)
            }
            NoteCommand::Get { id } => self.get(&id).map(NoteCommandResult::Note),
            NoteCommand::List => self.list().map(NoteCommandResult::Notes),
            NoteCommand::Rename { id, title } => {
                self.rename(&id, &title).map(NoteCommandResult::Note)
            }
            NoteCommand::SetBody { id, body } => {
                self.set_body(&id, &body).map(NoteCommandResult::Note)
            }
            NoteCommand::Delete { id } => self.delete(&id).map(NoteCommandResult::Deleted),
        }
    }

    fn migrate(&self) -> Result<(), NoteError> {
        self.database
            .with_connection(|connection| connection.execute_batch(NOTE_MIGRATIONS))?;
        let current = self.database.with_connection(|connection| {
            connection.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM tuilane_notes_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
        })?;
        if current > CURRENT_SCHEMA_VERSION {
            return Err(NoteError::UnsupportedSchemaVersion(current));
        }
        if current == 0 {
            self.database.transaction(|transaction| {
                transaction.execute_batch(include_str!("../migrations/0001_notes.sql"))?;
                transaction.execute(
                    "INSERT INTO tuilane_notes_migrations (version) VALUES (?1)",
                    [CURRENT_SCHEMA_VERSION],
                )?;
                Ok(())
            })?;
        }
        Ok(())
    }

    fn create(&self, title: NoteTitle, body: Markdown) -> Result<Note, NoteError> {
        let id = NoteId::new(Uuid::new_v4().to_string()).expect("UUID has canonical format");
        self.database
            .transaction(|transaction| {
                transaction.execute(
                    "INSERT INTO notes (id, title, body) VALUES (?1, ?2, ?3)",
                    params![id.as_str(), title.as_str(), body.as_str()],
                )?;
                transaction.query_row(
                    &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
                    [id.as_str()],
                    read_note,
                )
            })
            .map_err(NoteError::Storage)
    }

    fn get(&self, id: &NoteId) -> Result<Note, NoteError> {
        self.database
            .with_connection(|connection| {
                connection
                    .query_row(
                        &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
                        [id.as_str()],
                        read_note,
                    )
                    .optional()
            })?
            .ok_or_else(|| NoteError::NotFound(id.clone()))
    }

    fn list(&self) -> Result<Vec<Note>, NoteError> {
        self.database
            .with_connection(|connection| {
                let mut statement = connection.prepare(&format!(
                    "SELECT {NOTE_COLUMNS} FROM notes ORDER BY title COLLATE NOCASE, id"
                ))?;
                statement
                    .query_map([], read_note)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(NoteError::Storage)
    }

    fn rename(&self, id: &NoteId, title: &NoteTitle) -> Result<Note, NoteError> {
        let note = self.database.transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE notes SET title = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![title.as_str(), id.as_str()],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            transaction
                .query_row(
                    &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
                    [id.as_str()],
                    read_note,
                )
                .map(Some)
        })?;
        note.ok_or_else(|| NoteError::NotFound(id.clone()))
    }

    fn set_body(&self, id: &NoteId, body: &Markdown) -> Result<Note, NoteError> {
        let note = self.database.transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE notes SET body = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                params![body.as_str(), id.as_str()],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            transaction
                .query_row(
                    &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
                    [id.as_str()],
                    read_note,
                )
                .map(Some)
        })?;
        note.ok_or_else(|| NoteError::NotFound(id.clone()))
    }

    fn delete(&self, id: &NoteId) -> Result<NoteId, NoteError> {
        let deleted = self.database.transaction(|transaction| {
            transaction.execute("DELETE FROM notes WHERE id = ?1", [id.as_str()])
        })?;
        if deleted == 0 {
            return Err(NoteError::NotFound(id.clone()));
        }
        Ok(id.clone())
    }
}

fn read_note(row: &Row<'_>) -> rusqlite::Result<Note> {
    let id = row.get::<_, String>(0)?;
    let title = row.get::<_, String>(1)?;
    let body = row.get::<_, String>(2)?;
    let created_at = row.get::<_, String>(3)?;
    let updated_at = row.get::<_, String>(4)?;

    let id = NoteId::new(id).map_err(|error| conversion_error(0, error))?;
    let title = NoteTitle::new(title).map_err(|error| conversion_error(1, error))?;
    let body = Markdown::new(body).map_err(|error| conversion_error(2, error))?;

    Ok(Note::new(id, title, body, created_at, updated_at))
}

fn conversion_error(index: usize, error: impl Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}
