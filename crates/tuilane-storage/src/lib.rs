//! SQLite connection and transaction infrastructure shared by feature plugins.

use std::{
    fs, io,
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use rusqlite::{Connection, Transaction};

/// Shared handle to a configured local SQLite database.
#[derive(Clone)]
pub struct SqliteDatabase {
    connection: Arc<Mutex<Connection>>,
}

/// Failure while opening or using the SQLite database.
#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    Poisoned,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "database filesystem error: {error}"),
            Self::Sqlite(error) => write!(formatter, "SQLite error: {error}"),
            Self::Poisoned => formatter.write_str("SQLite connection lock is unavailable"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::Poisoned => None,
        }
    }
}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl<T> From<PoisonError<T>> for StorageError {
    fn from(_: PoisonError<T>) -> Self {
        Self::Poisoned
    }
}

impl SqliteDatabase {
    /// Opens a file-backed database, creating its parent directory when needed.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        Self::from_connection(connection, true)
    }

    /// Opens an in-memory database for isolated use and tests.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?, false)
    }

    fn from_connection(connection: Connection, file_backed: bool) -> Result<Self, StorageError> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        if file_backed {
            connection.pragma_update(None, "journal_mode", "WAL")?;
            connection.pragma_update(None, "synchronous", "NORMAL")?;
        }
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Runs a read or statement operation while holding the database connection.
    pub fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, StorageError> {
        let connection = self.connection.lock()?;
        operation(&connection).map_err(StorageError::Sqlite)
    }

    /// Runs an atomic transaction and commits only when the operation succeeds.
    pub fn transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> rusqlite::Result<T>,
    ) -> Result<T, StorageError> {
        let mut connection = self.connection.lock()?;
        let transaction = connection.transaction()?;
        let result = operation(&transaction)?;
        transaction.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_commits_success_and_rolls_back_failure() {
        let database = SqliteDatabase::open_in_memory().unwrap();
        database
            .transaction(|transaction| {
                transaction.execute_batch("CREATE TABLE entries (value INTEGER NOT NULL);")?;
                transaction.execute("INSERT INTO entries VALUES (1)", [])?;
                Ok(())
            })
            .unwrap();

        let result = database.transaction(|transaction| {
            transaction.execute("INSERT INTO entries VALUES (2)", [])?;
            Err::<(), _>(rusqlite::Error::InvalidQuery)
        });
        assert!(matches!(
            result,
            Err(StorageError::Sqlite(rusqlite::Error::InvalidQuery))
        ));

        let values = database
            .with_connection(|connection| {
                let mut statement =
                    connection.prepare("SELECT value FROM entries ORDER BY value")?;
                statement
                    .query_map([], |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap();
        assert_eq!(values, [1]);
    }
}
