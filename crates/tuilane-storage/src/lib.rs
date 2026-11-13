//! SQLite connection and transaction infrastructure shared by feature plugins.

use std::{
    fs,
    fs::OpenOptions,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, Transaction, backup::Backup};

const STORAGE_FORMAT_VERSION: i64 = 1;
const STORAGE_METADATA: &str = "CREATE TABLE IF NOT EXISTS tuilane_storage_metadata (key TEXT PRIMARY KEY NOT NULL, value INTEGER NOT NULL);";

/// Shared handle to a configured local SQLite database.
#[derive(Clone)]
pub struct SqliteDatabase {
    connection: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
}

/// Failure while opening or using the SQLite database.
#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    Poisoned,
    InvalidBackup(String),
    UnsupportedFormatVersion(i64),
    BackupDestinationIsDatabase,
    RestoreRecovery {
        restore: rusqlite::Error,
        recovery: rusqlite::Error,
    },
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "database filesystem error: {error}"),
            Self::Sqlite(error) => write!(formatter, "SQLite error: {error}"),
            Self::Poisoned => formatter.write_str("SQLite connection lock is unavailable"),
            Self::InvalidBackup(reason) => write!(formatter, "invalid tuilane backup: {reason}"),
            Self::UnsupportedFormatVersion(version) => write!(
                formatter,
                "database format version {version} is newer than supported version {STORAGE_FORMAT_VERSION}"
            ),
            Self::BackupDestinationIsDatabase => {
                formatter.write_str("backup destination cannot be the open database")
            }
            Self::RestoreRecovery { restore, recovery } => write!(
                formatter,
                "restore failed ({restore}) and the previous database could not be recovered ({recovery})"
            ),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::RestoreRecovery { restore, .. } => Some(restore),
            Self::Poisoned => None,
            Self::InvalidBackup(_)
            | Self::UnsupportedFormatVersion(_)
            | Self::BackupDestinationIsDatabase => None,
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
        if path == Path::new(":memory:") {
            Self::from_connection(connection, false, None)
        } else {
            Self::from_connection(connection, true, Some(fs::canonicalize(path)?))
        }
    }

    /// Opens an in-memory database for isolated use and tests.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?, false, None)
    }

    fn from_connection(
        connection: Connection,
        file_backed: bool,
        path: Option<PathBuf>,
    ) -> Result<Self, StorageError> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        initialize_storage_metadata(&connection)?;
        if file_backed {
            connection.pragma_update(None, "journal_mode", "WAL")?;
            connection.pragma_update(None, "synchronous", "NORMAL")?;
        }
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path,
        })
    }

    /// Writes a standalone SQLite backup without replacing an existing file.
    pub fn backup_to(&self, path: impl AsRef<Path>) -> Result<(), StorageError> {
        let path = path.as_ref();
        if let Some(database_path) = self.path.as_deref()
            && same_path(path, database_path)?
        {
            return Err(StorageError::BackupDestinationIsDatabase);
        }

        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        drop(file);

        let result = self.with_connection(|source| {
            let mut destination = Connection::open(path)?;
            copy_database(source, &mut destination)
        });
        if let Err(error) = result {
            let _ = fs::remove_file(path);
            return Err(error);
        }
        if let Err(error) = open_valid_backup(path) {
            let _ = fs::remove_file(path);
            return Err(error);
        }
        Ok(())
    }

    /// Restores a validated tuilane backup, recovering the current database if
    /// SQLite cannot finish the restore.
    pub fn restore_from(&self, path: impl AsRef<Path>) -> Result<(), StorageError> {
        let path = path.as_ref();
        if let Some(database_path) = self.path.as_deref()
            && same_path(path, database_path)?
        {
            return Err(StorageError::InvalidBackup(
                "the backup path is the active database".to_owned(),
            ));
        }
        let source = open_valid_backup(path)?;
        let mut connection = self.connection.lock()?;
        let recovery_path = reserve_temporary_backup()?;
        let recovery_result = (|| {
            let mut recovery = Connection::open(&recovery_path)?;
            copy_database(&connection, &mut recovery)
        })();
        if let Err(error) = recovery_result {
            let _ = fs::remove_file(&recovery_path);
            return Err(StorageError::Sqlite(error));
        }

        let restore_result = copy_database(&source, &mut connection);
        if let Err(restore) = restore_result {
            let recovery = Connection::open(&recovery_path)
                .and_then(|recovery| copy_database(&recovery, &mut connection));
            let _ = fs::remove_file(&recovery_path);
            return match recovery {
                Ok(()) => Err(StorageError::Sqlite(restore)),
                Err(recovery) => Err(StorageError::RestoreRecovery { restore, recovery }),
            };
        }
        fs::remove_file(recovery_path)?;
        Ok(())
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

fn initialize_storage_metadata(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(STORAGE_METADATA)?;
    connection.execute(
        "INSERT OR IGNORE INTO tuilane_storage_metadata (key, value) VALUES ('format_version', ?1)",
        [STORAGE_FORMAT_VERSION],
    )?;
    let version: i64 = connection.query_row(
        "SELECT value FROM tuilane_storage_metadata WHERE key = 'format_version'",
        [],
        |row| row.get(0),
    )?;
    if version != STORAGE_FORMAT_VERSION {
        return Err(StorageError::UnsupportedFormatVersion(version));
    }
    Ok(())
}

fn open_valid_backup(path: &Path) -> Result<Connection, StorageError> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| StorageError::InvalidBackup(error.to_string()))?;
    if integrity != "ok" {
        return Err(StorageError::InvalidBackup(integrity));
    }
    let version: i64 = connection
        .query_row(
            "SELECT value FROM tuilane_storage_metadata WHERE key = 'format_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::InvalidBackup(error.to_string()))?;
    if version != STORAGE_FORMAT_VERSION {
        return Err(StorageError::UnsupportedFormatVersion(version));
    }
    Ok(connection)
}

fn copy_database(source: &Connection, destination: &mut Connection) -> rusqlite::Result<()> {
    Backup::new(source, destination)?.run_to_completion(100, Duration::from_millis(10), None)
}

fn same_path(left: &Path, right: &Path) -> io::Result<bool> {
    let left = comparable_path(left)?;
    let right = comparable_path(right)?;
    Ok(left == right)
}

fn comparable_path(path: &Path) -> io::Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path);
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "database path has no filename")
    })?;
    Ok(fs::canonicalize(parent)?.join(filename))
}

fn reserve_temporary_backup() -> io::Result<PathBuf> {
    static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir();
    loop {
        let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "tuilane-restore-{}-{sequence}.sqlite3",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                drop(file);
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "tuilane-storage-test-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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

    #[test]
    fn backup_roundtrip_restores_records_and_keeps_the_backup_standalone() {
        let directory = TestDirectory::new();
        let database = SqliteDatabase::open(directory.path("workspace.sqlite3")).unwrap();
        database
            .transaction(|transaction| {
                transaction.execute_batch(
                    "CREATE TABLE entries (value TEXT NOT NULL); INSERT INTO entries VALUES ('saved');",
                )?;
                Ok(())
            })
            .unwrap();
        let backup_path = directory.path("backup.sqlite3");

        database.backup_to(&backup_path).unwrap();
        database
            .transaction(|transaction| {
                transaction.execute("INSERT INTO entries VALUES ('later')", [])?;
                Ok(())
            })
            .unwrap();
        database.restore_from(&backup_path).unwrap();

        let values = database
            .with_connection(|connection| {
                let mut statement =
                    connection.prepare("SELECT value FROM entries ORDER BY value")?;
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap();
        assert_eq!(values, ["saved"]);

        let restored_backup = Connection::open(&backup_path)
            .unwrap()
            .query_row("SELECT value FROM entries", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap();
        assert_eq!(restored_backup, "saved");
    }

    #[test]
    fn backup_refuses_the_live_database_and_existing_destinations() {
        let directory = TestDirectory::new();
        let database_path = directory.path("workspace.sqlite3");
        let database = SqliteDatabase::open(&database_path).unwrap();
        assert!(matches!(
            database.backup_to(&database_path),
            Err(StorageError::BackupDestinationIsDatabase)
        ));

        let backup_path = directory.path("existing.sqlite3");
        fs::write(&backup_path, b"keep this file").unwrap();
        assert!(matches!(
            database.backup_to(&backup_path),
            Err(StorageError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists
        ));
        assert_eq!(fs::read(backup_path).unwrap(), b"keep this file");
    }

    #[test]
    fn invalid_backup_is_rejected_without_changing_the_live_database() {
        let directory = TestDirectory::new();
        let database = SqliteDatabase::open(directory.path("workspace.sqlite3")).unwrap();
        database
            .transaction(|transaction| {
                transaction.execute_batch(
                    "CREATE TABLE entries (value TEXT NOT NULL); INSERT INTO entries VALUES ('preserve');",
                )?;
                Ok(())
            })
            .unwrap();
        let invalid_path = directory.path("unrelated.sqlite3");
        Connection::open(&invalid_path)
            .unwrap()
            .execute_batch("CREATE TABLE unrelated (value TEXT NOT NULL);")
            .unwrap();

        assert!(matches!(
            database.restore_from(&invalid_path),
            Err(StorageError::InvalidBackup(_))
        ));
        let value = database
            .with_connection(|connection| {
                connection.query_row("SELECT value FROM entries", [], |row| {
                    row.get::<_, String>(0)
                })
            })
            .unwrap();
        assert_eq!(value, "preserve");
    }
}
