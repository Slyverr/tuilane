use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

const STATE_VERSION: u32 = 1;
const MAX_STATE_BYTES: u64 = 16 * 1024;

/// Persisted enablement and permission grants for one installed plugin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginState {
    version: u32,
    enabled: bool,
    grants: BTreeSet<String>,
}

impl Default for PluginState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            enabled: false,
            grants: BTreeSet::new(),
        }
    }
}

impl PluginState {
    /// Returns whether the plugin is enabled for the next host startup or call.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the stored permission identifiers in stable order.
    pub fn grants(&self) -> impl Iterator<Item = &str> {
        self.grants.iter().map(String::as_str)
    }

    /// Returns whether the exact permission identifier has been granted.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.grants.contains(permission)
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn grant(&mut self, permission: impl Into<String>) -> bool {
        self.grants.insert(permission.into())
    }

    pub(crate) fn revoke(&mut self, permission: &str) -> bool {
        self.grants.remove(permission)
    }
}

#[derive(Debug, Error)]
pub(crate) enum StateError {
    #[error("could not inspect plugin state {path}: {source}")]
    Inspect { path: PathBuf, source: io::Error },
    #[error("plugin state {0} is not a regular file")]
    NotRegular(PathBuf),
    #[error("plugin state {path} exceeds {max} bytes")]
    TooLarge { path: PathBuf, max: u64 },
    #[error("could not read plugin state {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("plugin state {path} is invalid: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("plugin state {path} uses unsupported version {version}")]
    UnsupportedVersion { path: PathBuf, version: u32 },
    #[error("could not serialize plugin state: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("could not create plugin state directory {path}: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("could not write temporary plugin state {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("could not replace plugin state {path}: {source}")]
    Replace { path: PathBuf, source: io::Error },
}

pub(crate) fn read(path: &Path) -> Result<PluginState, StateError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(PluginState::default()),
        Err(source) => {
            return Err(StateError::Inspect {
                path: path.to_owned(),
                source,
            });
        }
    };
    if !metadata.file_type().is_file() {
        return Err(StateError::NotRegular(path.to_owned()));
    }
    if metadata.len() > MAX_STATE_BYTES {
        return Err(StateError::TooLarge {
            path: path.to_owned(),
            max: MAX_STATE_BYTES,
        });
    }

    let source = fs::read_to_string(path).map_err(|source| StateError::Read {
        path: path.to_owned(),
        source,
    })?;
    let state: PluginState = toml::from_str(&source).map_err(|source| StateError::Parse {
        path: path.to_owned(),
        source,
    })?;
    if state.version != STATE_VERSION {
        return Err(StateError::UnsupportedVersion {
            path: path.to_owned(),
            version: state.version,
        });
    }
    Ok(state)
}

pub(crate) fn write(path: &Path, state: &PluginState) -> Result<(), StateError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| StateError::CreateDirectory {
        path: parent.to_owned(),
        source,
    })?;

    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(StateError::NotRegular(path.to_owned()));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(StateError::Inspect {
                path: path.to_owned(),
                source,
            });
        }
    }

    let source = toml::to_string_pretty(state)?;
    let temporary = parent.join(format!(".plugin-state-{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| StateError::Write {
            path: temporary.clone(),
            source,
        })?;
    let result = (|| {
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok::<_, io::Error>(())
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(StateError::Replace {
            path: path.to_owned(),
            source,
        });
    }
    Ok(())
}
