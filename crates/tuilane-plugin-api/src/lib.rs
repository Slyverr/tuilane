//! Lightweight, host-neutral contracts shared by tuilane and extensions.

use std::{error::Error, fmt, str::FromStr};

pub mod ideas;
pub mod items;
pub mod notes;
pub mod projects;
pub mod tasks;

/// Typed notification published by a host or feature after a completed change.
pub trait Event: 'static {
    /// Stable name used for diagnostics.
    const NAME: &'static str;
}

/// Stable identifier for a host-discovered service capability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapabilityId {
    name: &'static str,
    version: u16,
}

impl CapabilityId {
    /// Creates a versioned capability identifier.
    pub const fn new(name: &'static str, version: u16) -> Self {
        Self { name, version }
    }

    /// Returns the namespaced service name.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Returns the contract version.
    pub const fn version(self) -> u16 {
        self.version
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.name, self.version)
    }
}

/// Typed request handled by a host-registered service provider.
pub trait ServiceRequest: 'static {
    /// The typed response returned by a compatible provider.
    type Response: 'static;

    /// Stable capability name and version for this request.
    const CAPABILITY: CapabilityId;
}

/// Typed command sent through the host's shared command dispatcher.
pub trait Command: 'static {
    /// The typed response returned by the command provider.
    type Response: 'static;

    /// Stable command name used for diagnostics.
    const NAME: &'static str;
}

/// Stable identifier for a host-registered view.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ViewId(Box<str>);

impl ViewId {
    /// Creates a view identifier from lowercase ASCII letters, digits, `.`, `-`, or `_`.
    pub fn new(value: impl AsRef<str>) -> Result<Self, ViewIdError> {
        let value = value.as_ref();
        if value.is_empty()
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
        {
            return Err(ViewIdError);
        }

        Ok(Self(value.into()))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ViewId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ViewId {
    type Err = ViewIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Error returned when a view identifier is empty or contains unsupported characters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewIdError;

impl fmt::Display for ViewIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "view identifiers must contain lowercase ASCII letters, digits, '.', '-' or '_'",
        )
    }
}

impl Error for ViewIdError {}

/// User-facing metadata for a registered workspace view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewDescriptor {
    id: ViewId,
    title: Box<str>,
    description: Box<str>,
}

impl ViewDescriptor {
    /// Creates metadata for a view.
    pub fn new(id: ViewId, title: impl Into<Box<str>>, description: impl Into<Box<str>>) -> Self {
        Self {
            id,
            title: title.into(),
            description: description.into(),
        }
    }

    /// Returns the stable view identifier.
    pub fn id(&self) -> &ViewId {
        &self.id
    }

    /// Returns the title shown in workspace navigation.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the short description shown in the workspace shell.
    pub fn description(&self) -> &str {
        &self.description
    }
}
