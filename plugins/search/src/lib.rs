//! Host-mediated full-text search across first-party workspace items.

mod view;

pub use view::SearchView;

use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use rusqlite::params;
use tuilane_core::{
    CommandDispatcher, CommandRegistrationError, EventBus, EventSubscription, ServiceBroker,
    ServiceBrokerError,
};
use tuilane_plugin_api::{
    Event,
    ideas::IdeasChangedV1,
    items::{ItemRef, ItemSummary, SearchDocument},
    notes::NotesChangedV1,
    projects::ProjectsChangedV1,
    search::{
        IdeaSearchDocumentsV1, NoteSearchDocumentsV1, ProjectSearchDocumentsV1, SearchCommand,
        SearchCommandResult, SearchQuery, SearchResult, TaskSearchDocumentsV1,
    },
    tasks::TasksChangedV1,
};
use tuilane_storage::{SqliteDatabase, StorageError};

const SEARCH_MIGRATIONS: &str =
    "CREATE TABLE IF NOT EXISTS tuilane_search_migrations (version INTEGER PRIMARY KEY NOT NULL);";
const CURRENT_SCHEMA_VERSION: i64 = 1;
const SEARCH_INDEX: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS tuilane_search_index USING fts5(item_ref UNINDEXED, kind UNINDEXED, title, content);";

/// Errors returned by workspace search operations.
#[derive(Debug)]
pub enum SearchError {
    Storage(StorageError),
    Service(ServiceBrokerError),
    ServiceBrokerBusy,
    CommandRegistration(CommandRegistrationError),
    UnsupportedSchemaVersion(i64),
    InvalidStoredReference(String),
    InvalidStoredSummary(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::Service(error) => write!(formatter, "search provider failed: {error}"),
            Self::ServiceBrokerBusy => formatter.write_str("search providers are busy"),
            Self::CommandRegistration(error) => error.fmt(formatter),
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "search schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
            Self::InvalidStoredReference(reference) => {
                write!(
                    formatter,
                    "search index contains invalid item reference '{reference}'"
                )
            }
            Self::InvalidStoredSummary(message) => {
                write!(
                    formatter,
                    "search index contains invalid item metadata: {message}"
                )
            }
        }
    }
}

impl Error for SearchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Service(error) => Some(error),
            Self::CommandRegistration(error) => Some(error),
            Self::ServiceBrokerBusy
            | Self::UnsupportedSchemaVersion(_)
            | Self::InvalidStoredReference(_)
            | Self::InvalidStoredSummary(_) => None,
        }
    }
}

impl From<StorageError> for SearchError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<ServiceBrokerError> for SearchError {
    fn from(error: ServiceBrokerError) -> Self {
        Self::Service(error)
    }
}

/// Search index owner and change-event subscriptions.
pub struct SearchPlugin {
    _subscriptions: Vec<EventSubscription>,
}

impl SearchPlugin {
    /// Builds the initial index, registers the shared search command, and
    /// refreshes the index after first-party item changes.
    pub fn register(
        database: SqliteDatabase,
        services: Rc<RefCell<ServiceBroker>>,
        commands: &mut CommandDispatcher,
        events: &EventBus,
    ) -> Result<Self, SearchError> {
        migrate(&database)?;
        refresh_index(&database, &services)?;

        let stale = Rc::new(RefCell::new(None::<String>));
        let command_database = database.clone();
        let command_services = Rc::clone(&services);
        let command_stale = Rc::clone(&stale);
        commands
            .register(
                move |command: SearchCommand| -> Result<SearchCommandResult, SearchError> {
                    if command_stale.borrow().is_some() {
                        refresh_index(&command_database, &command_services)?;
                        command_stale.borrow_mut().take();
                    }
                    search(&command_database, &command)
                },
            )
            .map_err(SearchError::CommandRegistration)?;

        let subscriptions = vec![
            subscribe_and_refresh::<TasksChangedV1>(
                events,
                database.clone(),
                Rc::clone(&services),
                Rc::clone(&stale),
            ),
            subscribe_and_refresh::<ProjectsChangedV1>(
                events,
                database.clone(),
                Rc::clone(&services),
                Rc::clone(&stale),
            ),
            subscribe_and_refresh::<NotesChangedV1>(
                events,
                database.clone(),
                Rc::clone(&services),
                Rc::clone(&stale),
            ),
            subscribe_and_refresh::<IdeasChangedV1>(events, database, services, stale),
        ];
        Ok(Self {
            _subscriptions: subscriptions,
        })
    }
}

fn subscribe_and_refresh<E: Event>(
    events: &EventBus,
    database: SqliteDatabase,
    services: Rc<RefCell<ServiceBroker>>,
    stale: Rc<RefCell<Option<String>>>,
) -> EventSubscription {
    events.subscribe(move |_: &E| {
        let error = refresh_index(&database, &services)
            .err()
            .map(|error| error.to_string());
        *stale.borrow_mut() = error;
    })
}

fn migrate(database: &SqliteDatabase) -> Result<(), SearchError> {
    let current = database.transaction(|transaction| {
        transaction.execute_batch(SEARCH_MIGRATIONS)?;
        let current: Option<i64> = transaction.query_row(
            "SELECT MAX(version) FROM tuilane_search_migrations",
            [],
            |row| row.get(0),
        )?;
        let current = current.unwrap_or(0);
        if current < 1 {
            transaction.execute_batch(SEARCH_INDEX)?;
            transaction.execute(
                "INSERT INTO tuilane_search_migrations (version) VALUES (1)",
                [],
            )?;
        }
        Ok(current)
    })?;
    if current > CURRENT_SCHEMA_VERSION {
        return Err(SearchError::UnsupportedSchemaVersion(current));
    }
    Ok(())
}

fn refresh_index(
    database: &SqliteDatabase,
    services: &Rc<RefCell<ServiceBroker>>,
) -> Result<(), SearchError> {
    let documents = {
        let mut broker = services
            .try_borrow_mut()
            .map_err(|_| SearchError::ServiceBrokerBusy)?;
        let mut documents = broker.call::<TaskSearchDocumentsV1>(TaskSearchDocumentsV1)?;
        documents.extend(broker.call::<ProjectSearchDocumentsV1>(ProjectSearchDocumentsV1)?);
        documents.extend(broker.call::<NoteSearchDocumentsV1>(NoteSearchDocumentsV1)?);
        documents.extend(broker.call::<IdeaSearchDocumentsV1>(IdeaSearchDocumentsV1)?);
        documents
    };

    database.transaction(|transaction| {
        transaction.execute("DELETE FROM tuilane_search_index", [])?;
        let mut statement = transaction.prepare(
            "INSERT INTO tuilane_search_index (item_ref, kind, title, content) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for document in &documents {
            insert_document(&mut statement, document)?;
        }
        Ok(())
    })?;
    Ok(())
}

fn insert_document(
    statement: &mut rusqlite::Statement<'_>,
    document: &SearchDocument,
) -> rusqlite::Result<()> {
    let item = document.item();
    statement.execute(params![
        item.reference().to_string(),
        item.reference().kind(),
        item.title(),
        document.content(),
    ])?;
    Ok(())
}

fn search(
    database: &SqliteDatabase,
    command: &SearchCommand,
) -> Result<SearchCommandResult, SearchError> {
    let expression = fts_expression(command.query());
    if expression.is_empty() {
        return Ok(SearchCommandResult::Results(Vec::new()));
    }
    let results = database.with_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT item_ref, title, snippet(tuilane_search_index, 3, '[', ']', '…', 12) FROM tuilane_search_index WHERE tuilane_search_index MATCH ?1 AND (?2 IS NULL OR kind = ?2) ORDER BY rank LIMIT 100",
        )?;
        statement
            .query_map(params![expression, command.kind()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?;
    let results = results
        .into_iter()
        .map(|(reference, title, excerpt)| {
            let item = reference
                .parse::<ItemRef>()
                .map_err(|_| SearchError::InvalidStoredReference(reference.clone()))?;
            let item = ItemSummary::new(item, title)
                .map_err(|error| SearchError::InvalidStoredSummary(error.to_string()))?;
            Ok(SearchResult::new(item, excerpt))
        })
        .collect::<Result<Vec<_>, SearchError>>()?;
    Ok(SearchCommandResult::Results(results))
}

fn fts_expression(query: &SearchQuery) -> String {
    query
        .as_str()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{token}\""))
        .collect::<Vec<_>>()
        .join(" ")
}
