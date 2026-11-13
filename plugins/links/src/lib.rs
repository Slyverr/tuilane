//! First-party directed item links and file/URL references.

mod view;

pub use view::LinksView;

use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use rusqlite::{Row, params, types::Type};
use tuilane_core::{
    CommandDispatcher, CommandRegistrationError, EventBus, ServiceBroker, ServiceRegistrationError,
};
use tuilane_plugin_api::ideas::IdeaByIdV1;
use tuilane_plugin_api::items::{ItemRef, ItemReferenceError, ItemRemovedV1, ItemSummary};
use tuilane_plugin_api::links::{
    ExternalReference, ExternalReferenceError, ItemLink, LinkCommand, LinkCommandResult,
    LinksChangedV1, ReferencesForItemV1,
};
use tuilane_plugin_api::notes::NoteByIdV1;
use tuilane_plugin_api::projects::ProjectByIdV1;
use tuilane_plugin_api::tasks::TaskByIdV1;
use tuilane_storage::{SqliteDatabase, StorageError};

const LINK_MIGRATIONS: &str =
    "CREATE TABLE IF NOT EXISTS tuilane_links_migrations (version INTEGER PRIMARY KEY NOT NULL);";
const CURRENT_SCHEMA_VERSION: i64 = 1;

/// Errors returned by the Links feature.
#[derive(Debug)]
pub enum LinkError {
    Storage(StorageError),
    Service(String),
    ServiceBrokerBusy,
    ItemNotFound(ItemRef),
    UnsupportedItemKind(String),
    InvalidItemId(ItemRef),
    SelfLink,
    LinkNotFound(ItemLink),
    ReferenceNotFound(ItemRef, ExternalReference),
    UnsupportedSchemaVersion(i64),
    InvalidStoredValue(String),
    InvalidItemReference(ItemReferenceError),
    InvalidExternalReference(ExternalReferenceError),
}

impl fmt::Display for LinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::Service(message) => write!(formatter, "item lookup failed: {message}"),
            Self::ServiceBrokerBusy => formatter.write_str("item service broker is busy"),
            Self::ItemNotFound(item) => write!(formatter, "item '{item}' was not found"),
            Self::UnsupportedItemKind(kind) => {
                write!(formatter, "item kind '{kind}' is unsupported")
            }
            Self::InvalidItemId(item) => {
                write!(formatter, "item '{item}' has an invalid identifier")
            }
            Self::SelfLink => formatter.write_str("an item cannot link to itself"),
            Self::LinkNotFound(link) => {
                write!(
                    formatter,
                    "link '{} -> {}' was not found",
                    link.source(),
                    link.target()
                )
            }
            Self::ReferenceNotFound(owner, reference) => write!(
                formatter,
                "reference '{}' for item '{}' was not found",
                reference.value(),
                owner
            ),
            Self::UnsupportedSchemaVersion(version) => write!(
                formatter,
                "links database schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ),
            Self::InvalidStoredValue(value) => {
                write!(
                    formatter,
                    "links database contains an invalid value: {value}"
                )
            }
            Self::InvalidItemReference(error) => error.fmt(formatter),
            Self::InvalidExternalReference(error) => error.fmt(formatter),
        }
    }
}

impl Error for LinkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::InvalidItemReference(error) => Some(error),
            Self::InvalidExternalReference(error) => Some(error),
            Self::Service(_)
            | Self::ServiceBrokerBusy
            | Self::ItemNotFound(_)
            | Self::UnsupportedItemKind(_)
            | Self::InvalidItemId(_)
            | Self::SelfLink
            | Self::LinkNotFound(_)
            | Self::ReferenceNotFound(_, _)
            | Self::UnsupportedSchemaVersion(_)
            | Self::InvalidStoredValue(_) => None,
        }
    }
}

impl From<StorageError> for LinkError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<ItemReferenceError> for LinkError {
    fn from(error: ItemReferenceError) -> Self {
        Self::InvalidItemReference(error)
    }
}

impl From<ExternalReferenceError> for LinkError {
    fn from(error: ExternalReferenceError) -> Self {
        Self::InvalidExternalReference(error)
    }
}

/// Errors returned while registering Links with the host.
#[derive(Debug)]
pub enum LinkRegistrationError {
    Command(CommandRegistrationError),
    Service(ServiceRegistrationError),
    ServiceBrokerBusy,
}

impl fmt::Display for LinkRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(error) => error.fmt(formatter),
            Self::Service(error) => error.fmt(formatter),
            Self::ServiceBrokerBusy => formatter.write_str("item service broker is busy"),
        }
    }
}

impl Error for LinkRegistrationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Service(error) => Some(error),
            Self::ServiceBrokerBusy => None,
        }
    }
}

/// First-party Links feature backed by its own tables in the shared SQLite database.
#[derive(Clone)]
pub struct LinksPlugin {
    database: SqliteDatabase,
}

impl LinksPlugin {
    /// Opens the link schema and applies any required migrations.
    pub fn new(database: SqliteDatabase) -> Result<Self, LinkError> {
        let plugin = Self { database };
        plugin.migrate()?;
        Ok(plugin)
    }

    /// Registers link commands, the external-reference lookup service, and removal cleanup.
    pub fn register(
        self,
        commands: &mut CommandDispatcher,
        services: Rc<RefCell<ServiceBroker>>,
        events: EventBus,
    ) -> Result<(), LinkRegistrationError> {
        let reference_provider = self.clone();
        services
            .try_borrow_mut()
            .map_err(|_| LinkRegistrationError::ServiceBrokerBusy)?
            .register(move |request: ReferencesForItemV1| {
                reference_provider.references(request.item())
            })
            .map_err(LinkRegistrationError::Service)?;

        let cleanup_database = self.database.clone();
        let cleanup_events = events.clone();
        let cleanup_subscription = events.subscribe(move |removed: &ItemRemovedV1| {
            let item = removed.item();
            match cleanup_database.transaction(|transaction| {
                let links = transaction.execute(
                    "DELETE FROM item_links WHERE (source_kind = ?1 AND source_id = ?2) OR (target_kind = ?1 AND target_id = ?2)",
                    params![item.kind(), item.id()],
                )?;
                let references = transaction.execute(
                    "DELETE FROM external_references WHERE owner_kind = ?1 AND owner_id = ?2",
                    params![item.kind(), item.id()],
                )?;
                Ok(links + references > 0)
            }) {
                Ok(true) => cleanup_events.publish(LinksChangedV1),
                Ok(false) => {}
                Err(error) => {
                    eprintln!("tuilane: failed to clean links for removed item '{item}': {error}");
                }
            }
        });

        let command_provider = self;
        let command_events = events;
        commands
            .register(
                move |command: LinkCommand| -> Result<LinkCommandResult, LinkError> {
                    let result = command_provider.execute(command, &services)?;
                    if matches!(
                        &result,
                        LinkCommandResult::Link(_)
                            | LinkCommandResult::Removed(_)
                            | LinkCommandResult::Reference(_)
                    ) {
                        command_events.publish(LinksChangedV1);
                    }
                    let _keep_cleanup_subscription_alive = &cleanup_subscription;
                    Ok(result)
                },
            )
            .map_err(LinkRegistrationError::Command)?;
        Ok(())
    }

    fn execute(
        &self,
        command: LinkCommand,
        services: &Rc<RefCell<ServiceBroker>>,
    ) -> Result<LinkCommandResult, LinkError> {
        match command {
            LinkCommand::Add { source, target } => {
                if source == target {
                    return Err(LinkError::SelfLink);
                }
                self.resolve_item(services, &source)?;
                self.resolve_item(services, &target)?;
                self.database.transaction(|transaction| {
                    transaction.execute(
                        "INSERT INTO item_links (source_kind, source_id, target_kind, target_id) VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
                        params![source.kind(), source.id(), target.kind(), target.id()],
                    )?;
                    Ok(())
                })?;
                Ok(LinkCommandResult::Link(ItemLink::new(source, target)))
            }
            LinkCommand::Remove { source, target } => {
                let link = ItemLink::new(source, target);
                let deleted = self.database.transaction(|transaction| {
                    transaction.execute(
                        "DELETE FROM item_links WHERE source_kind = ?1 AND source_id = ?2 AND target_kind = ?3 AND target_id = ?4",
                        params![link.source().kind(), link.source().id(), link.target().kind(), link.target().id()],
                    )
                })?;
                if deleted == 0 {
                    return Err(LinkError::LinkNotFound(link));
                }
                Ok(LinkCommandResult::Removed(link))
            }
            LinkCommand::Outgoing { item } => {
                self.resolve_item(services, &item)?;
                let references = self.query_item_links(
                    "SELECT target_kind, target_id FROM item_links WHERE source_kind = ?1 AND source_id = ?2 ORDER BY target_kind, target_id",
                    &item,
                )?;
                self.resolve_all(services, references)
                    .map(LinkCommandResult::Items)
            }
            LinkCommand::Backlinks { item } => {
                self.resolve_item(services, &item)?;
                let references = self.query_item_links(
                    "SELECT source_kind, source_id FROM item_links WHERE target_kind = ?1 AND target_id = ?2 ORDER BY source_kind, source_id",
                    &item,
                )?;
                self.resolve_all(services, references)
                    .map(LinkCommandResult::Items)
            }
            LinkCommand::AddReference { owner, reference } => {
                self.resolve_item(services, &owner)?;
                self.database.transaction(|transaction| {
                    transaction.execute(
                        "INSERT INTO external_references (owner_kind, owner_id, reference_kind, value) VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
                        params![owner.kind(), owner.id(), reference.kind().as_str(), reference.value()],
                    )?;
                    Ok(())
                })?;
                Ok(LinkCommandResult::Reference(reference))
            }
            LinkCommand::RemoveReference { owner, reference } => {
                let deleted = self.database.transaction(|transaction| {
                    transaction.execute(
                        "DELETE FROM external_references WHERE owner_kind = ?1 AND owner_id = ?2 AND reference_kind = ?3 AND value = ?4",
                        params![owner.kind(), owner.id(), reference.kind().as_str(), reference.value()],
                    )
                })?;
                if deleted == 0 {
                    return Err(LinkError::ReferenceNotFound(owner, reference));
                }
                Ok(LinkCommandResult::Reference(reference))
            }
            LinkCommand::References { owner } => {
                self.resolve_item(services, &owner)?;
                self.references(&owner).map(LinkCommandResult::References)
            }
        }
    }

    fn resolve_item(
        &self,
        services: &Rc<RefCell<ServiceBroker>>,
        item: &ItemRef,
    ) -> Result<ItemSummary, LinkError> {
        let mut services = services
            .try_borrow_mut()
            .map_err(|_| LinkError::ServiceBrokerBusy)?;
        let summary = match item.kind() {
            "task" => {
                let id = tuilane_plugin_api::tasks::TaskId::new(item.id())
                    .map_err(|_| LinkError::InvalidItemId(item.clone()))?;
                services.call(TaskByIdV1::new(id))
            }
            "project" => {
                let id = tuilane_plugin_api::projects::ProjectId::new(item.id())
                    .map_err(|_| LinkError::InvalidItemId(item.clone()))?;
                services.call(ProjectByIdV1::new(id))
            }
            "note" => {
                let id = tuilane_plugin_api::notes::NoteId::new(item.id())
                    .map_err(|_| LinkError::InvalidItemId(item.clone()))?;
                services.call(NoteByIdV1::new(id))
            }
            "idea" => {
                let id = tuilane_plugin_api::ideas::IdeaId::new(item.id())
                    .map_err(|_| LinkError::InvalidItemId(item.clone()))?;
                services.call(IdeaByIdV1::new(id))
            }
            kind => return Err(LinkError::UnsupportedItemKind(kind.to_owned())),
        }
        .map_err(|error| LinkError::Service(error.to_string()))?;
        summary.ok_or_else(|| LinkError::ItemNotFound(item.clone()))
    }

    fn resolve_all(
        &self,
        services: &Rc<RefCell<ServiceBroker>>,
        items: Vec<ItemRef>,
    ) -> Result<Vec<ItemSummary>, LinkError> {
        items
            .iter()
            .map(|item| self.resolve_item(services, item))
            .collect()
    }

    fn query_item_links(&self, query: &str, item: &ItemRef) -> Result<Vec<ItemRef>, LinkError> {
        self.database
            .with_connection(|connection| {
                let mut statement = connection.prepare(query)?;
                statement
                    .query_map(params![item.kind(), item.id()], |row| {
                        item_reference(row, 0, 1)
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(LinkError::Storage)
    }

    fn references(&self, owner: &ItemRef) -> Result<Vec<ExternalReference>, LinkError> {
        self.database
            .with_connection(|connection| {
                let mut statement = connection.prepare(
                    "SELECT reference_kind, value FROM external_references WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY reference_kind, value",
                )?;
                statement
                    .query_map(params![owner.kind(), owner.id()], read_reference)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(LinkError::Storage)
    }

    fn migrate(&self) -> Result<(), LinkError> {
        self.database
            .with_connection(|connection| connection.execute_batch(LINK_MIGRATIONS))?;
        let current = self.database.with_connection(|connection| {
            connection.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM tuilane_links_migrations",
                [],
                |row| row.get::<_, i64>(0),
            )
        })?;
        if current > CURRENT_SCHEMA_VERSION {
            return Err(LinkError::UnsupportedSchemaVersion(current));
        }
        if current == 0 {
            self.database.transaction(|transaction| {
                transaction.execute_batch(include_str!("../migrations/0001_links.sql"))?;
                transaction.execute(
                    "INSERT INTO tuilane_links_migrations (version) VALUES (?1)",
                    [CURRENT_SCHEMA_VERSION],
                )?;
                Ok(())
            })?;
        }
        Ok(())
    }
}

fn item_reference(row: &Row<'_>, kind_index: usize, id_index: usize) -> rusqlite::Result<ItemRef> {
    let kind = row.get::<_, String>(kind_index)?;
    let id = row.get::<_, String>(id_index)?;
    ItemRef::new(kind, id).map_err(|error| conversion_error(kind_index, error))
}

fn read_reference(row: &Row<'_>) -> rusqlite::Result<ExternalReference> {
    let kind = row.get::<_, String>(0)?;
    let value = row.get::<_, String>(1)?;
    let reference = match kind.as_str() {
        "file" => ExternalReference::file(value),
        "url" => ExternalReference::url(value),
        _ => {
            return Err(conversion_error(
                0,
                LinkError::InvalidStoredValue(format!("unsupported reference kind '{kind}'")),
            ));
        }
    };
    reference.map_err(|error| conversion_error(1, error))
}

fn conversion_error(index: usize, error: impl Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}
