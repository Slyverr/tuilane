//! Lightweight, host-neutral contracts shared by tuilane and extensions.

use std::{error::Error, fmt, str::FromStr};

pub mod ideas;
pub mod items;
pub mod links;
pub mod notes;
pub mod plugin;
pub mod projects;
pub mod search;
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
///
/// Its `Debug` representation is included in local command previews after control
/// characters are escaped. Do not store secrets or private implementation state
/// in a command value.
pub trait Command: fmt::Debug + 'static {
    /// The typed response returned by the command provider.
    type Response: 'static;

    /// Stable command name used for diagnostics.
    const NAME: &'static str;

    /// Available operation descriptors for this command type.
    const DESCRIPTORS: &'static [CommandDescriptor];

    /// Returns the descriptor for this particular operation.
    fn descriptor(&self) -> &'static CommandDescriptor;

    /// Returns the permission required for this particular operation.
    fn required_permission(&self) -> CommandPermission {
        self.descriptor().required_permission()
    }
}

/// Stable, feature-scoped permission required by a command operation.
///
/// Permission names are declared by trusted command contracts, for example
/// `tasks.read` or `notes.delete`. They are not caller-provided input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommandPermission(&'static str);

impl CommandPermission {
    /// Creates a permission identifier for a trusted command contract.
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// Returns the stable permission name.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Describes the observable effect of running a command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandEffect {
    /// Reads or computes information without changing workspace state.
    ReadOnly,
    /// Changes workspace state without deleting an existing record.
    Mutating,
    /// Deletes a record or removes an existing relationship.
    Destructive,
}

/// Host-neutral discovery metadata for one command operation.
///
/// The input and output descriptions are logical typed contracts, not a wire
/// format. Adapters must validate external data into the command's Rust types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandDescriptor {
    id: &'static str,
    summary: &'static str,
    input: &'static str,
    output: &'static str,
    effect: CommandEffect,
    required_permission: CommandPermission,
}

impl CommandDescriptor {
    /// Creates metadata for one stable operation.
    pub const fn new(
        id: &'static str,
        summary: &'static str,
        input: &'static str,
        output: &'static str,
        effect: CommandEffect,
        required_permission: CommandPermission,
    ) -> Self {
        Self {
            id,
            summary,
            input,
            output,
            effect,
            required_permission,
        }
    }

    /// Returns the stable namespaced operation identifier.
    pub const fn id(self) -> &'static str {
        self.id
    }

    /// Returns the short operation description.
    pub const fn summary(self) -> &'static str {
        self.summary
    }

    /// Returns the logical input contract description.
    pub const fn input(self) -> &'static str {
        self.input
    }

    /// Returns the logical output contract description.
    pub const fn output(self) -> &'static str {
        self.output
    }

    /// Returns the declared effect.
    pub const fn effect(self) -> CommandEffect {
        self.effect
    }

    /// Returns the permission required to run the operation.
    pub const fn required_permission(self) -> CommandPermission {
        self.required_permission
    }
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
