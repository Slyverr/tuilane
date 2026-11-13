//! Public idea models and command/service contracts.

use std::{error::Error, fmt, str::FromStr};

use crate::{CapabilityId, Command, CommandPermission, Event, ServiceRequest, items::ItemSummary};

/// Stable identifier for an idea.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IdeaId(Box<str>);

impl IdeaId {
    /// Creates a canonical UUID-shaped idea identifier.
    pub fn new(value: impl AsRef<str>) -> Result<Self, IdeaIdError> {
        let value = value.as_ref();
        let valid = value.len() == 36
            && value.bytes().enumerate().all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            });
        if !valid {
            return Err(IdeaIdError);
        }
        Ok(Self(value.to_ascii_lowercase().into()))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdeaId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for IdeaId {
    type Err = IdeaIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Error returned for a malformed idea identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdeaIdError;

impl fmt::Display for IdeaIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("idea identifiers must use the canonical UUID format")
    }
}

impl Error for IdeaIdError {}

/// Validated nonempty idea title, limited to 200 Unicode scalar values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeaTitle(Box<str>);

impl IdeaTitle {
    /// Creates a trimmed title and rejects empty, oversized, or control-character input.
    pub fn new(value: impl AsRef<str>) -> Result<Self, IdeaTitleError> {
        let value = value.as_ref().trim();
        if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
            return Err(IdeaTitleError);
        }
        Ok(Self(value.into()))
    }

    /// Returns the validated idea title.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdeaTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Error returned for a blank, oversized, or unsupported idea title.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdeaTitleError;

impl fmt::Display for IdeaTitleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("idea titles must contain 1 to 200 characters and no controls")
    }
}

impl Error for IdeaTitleError {}

/// Durable idea record returned by the Ideas feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Idea {
    id: IdeaId,
    title: IdeaTitle,
    created_at: Box<str>,
    updated_at: Box<str>,
}

impl Idea {
    /// Creates an idea record from validated fields.
    pub fn new(
        id: IdeaId,
        title: IdeaTitle,
        created_at: impl Into<Box<str>>,
        updated_at: impl Into<Box<str>>,
    ) -> Self {
        Self {
            id,
            title,
            created_at: created_at.into(),
            updated_at: updated_at.into(),
        }
    }

    /// Returns the stable idea identifier.
    pub fn id(&self) -> &IdeaId {
        &self.id
    }

    /// Returns the validated idea title.
    pub fn title(&self) -> &IdeaTitle {
        &self.title
    }

    /// Returns the creation timestamp stored by SQLite.
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    /// Returns the most recent update timestamp stored by SQLite.
    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }
}

/// Typed idea operation routed through the host command dispatcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdeaCommand {
    Create { title: IdeaTitle },
    Get { id: IdeaId },
    List,
    Rename { id: IdeaId, title: IdeaTitle },
    Delete { id: IdeaId },
}

/// Typed idea command response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdeaCommandResult {
    Idea(Idea),
    Ideas(Vec<Idea>),
    Deleted(IdeaId),
}

impl Command for IdeaCommand {
    type Response = IdeaCommandResult;

    const NAME: &'static str = "ideas";

    fn required_permission(&self) -> CommandPermission {
        match self {
            Self::Get { .. } | Self::List => CommandPermission::new("ideas.read"),
            Self::Create { .. } | Self::Rename { .. } => CommandPermission::new("ideas.write"),
            Self::Delete { .. } => CommandPermission::new("ideas.delete"),
        }
    }
}

/// Version 1 lookup for an idea's stable cross-feature item reference and title.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeaByIdV1 {
    id: IdeaId,
}

impl IdeaByIdV1 {
    /// Creates a lookup for one validated idea ID.
    pub fn new(id: IdeaId) -> Self {
        Self { id }
    }

    /// Returns the requested idea ID.
    pub fn id(&self) -> &IdeaId {
        &self.id
    }
}

impl ServiceRequest for IdeaByIdV1 {
    type Response = Option<ItemSummary>;

    const CAPABILITY: CapabilityId = CapabilityId::new("ideas.item.by_id", 1);
}

/// Notification emitted after an idea has changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdeasChangedV1;

impl Event for IdeasChangedV1 {
    const NAME: &'static str = "ideas.changed@1";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idea_titles_trim_and_validate_input() {
        assert_eq!(
            IdeaTitle::new("  Prototype  ").unwrap().as_str(),
            "Prototype"
        );
        assert!(IdeaTitle::new(" ").is_err());
        assert!(IdeaTitle::new("bad\ntitle").is_err());
    }

    #[test]
    fn idea_ids_require_canonical_uuid_shape() {
        let id = IdeaId::new("550E8400-E29B-41D4-A716-446655440000").unwrap();
        assert_eq!(id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
        assert!(IdeaId::new("not-an-idea-id").is_err());
    }

    #[test]
    fn idea_lookup_uses_a_versioned_item_capability() {
        let id = IdeaId::new("00000000-0000-4000-8000-000000000001").unwrap();
        let request = IdeaByIdV1::new(id.clone());

        assert_eq!(request.id(), &id);
        assert_eq!(
            IdeaByIdV1::CAPABILITY,
            CapabilityId::new("ideas.item.by_id", 1)
        );
    }
}
