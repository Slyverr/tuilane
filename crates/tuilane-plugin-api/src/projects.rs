//! Public project models and command/service contracts.

use std::{error::Error, fmt, str::FromStr};

use crate::{CapabilityId, Command, CommandPermission, Event, ServiceRequest, items::ItemSummary};

/// Stable identifier for a project.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectId(Box<str>);

impl ProjectId {
    /// Creates a canonical UUID-shaped project identifier.
    pub fn new(value: impl AsRef<str>) -> Result<Self, ProjectIdError> {
        let value = value.as_ref();
        let valid = value.len() == 36
            && value.bytes().enumerate().all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            });
        if !valid {
            return Err(ProjectIdError);
        }
        Ok(Self(value.to_ascii_lowercase().into()))
    }

    /// Returns the project identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ProjectId {
    type Err = ProjectIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Error returned when a project identifier is not canonical UUID text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectIdError;

impl fmt::Display for ProjectIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project identifiers must use the canonical UUID format")
    }
}

impl Error for ProjectIdError {}

/// Validated nonempty project title, limited to 200 Unicode scalar values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectTitle(Box<str>);

impl ProjectTitle {
    /// Creates a trimmed title and rejects empty, oversized, or control-character input.
    pub fn new(value: impl AsRef<str>) -> Result<Self, ProjectTitleError> {
        let value = value.as_ref().trim();
        if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
            return Err(ProjectTitleError);
        }
        Ok(Self(value.into()))
    }

    /// Returns the validated title.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Error returned for a blank, oversized, or unsupported project title.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectTitleError;

impl fmt::Display for ProjectTitleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project titles must contain 1 to 200 characters and no controls")
    }
}

impl Error for ProjectTitleError {}

/// Durable project record returned by the Projects feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    id: ProjectId,
    title: ProjectTitle,
    created_at: Box<str>,
    updated_at: Box<str>,
}

impl Project {
    /// Creates a project record from validated fields.
    pub fn new(
        id: ProjectId,
        title: ProjectTitle,
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

    /// Returns the stable project identifier.
    pub fn id(&self) -> &ProjectId {
        &self.id
    }

    /// Returns the validated project title.
    pub fn title(&self) -> &ProjectTitle {
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

/// Typed project operation routed through the host command dispatcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectCommand {
    Create { title: ProjectTitle },
    Get { id: ProjectId },
    List,
    Rename { id: ProjectId, title: ProjectTitle },
    Delete { id: ProjectId },
}

/// Typed project command response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectCommandResult {
    Project(Project),
    Projects(Vec<Project>),
    Deleted(ProjectId),
}

impl Command for ProjectCommand {
    type Response = ProjectCommandResult;

    const NAME: &'static str = "projects";

    fn required_permission(&self) -> CommandPermission {
        match self {
            Self::Get { .. } | Self::List => CommandPermission::new("projects.read"),
            Self::Create { .. } | Self::Rename { .. } => CommandPermission::new("projects.write"),
            Self::Delete { .. } => CommandPermission::new("projects.delete"),
        }
    }
}

/// Version 1 lookup for a project's stable cross-feature item reference and title.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectByIdV1 {
    id: ProjectId,
}

impl ProjectByIdV1 {
    /// Creates a lookup for one validated project ID.
    pub fn new(id: ProjectId) -> Self {
        Self { id }
    }

    /// Returns the requested project ID.
    pub fn id(&self) -> &ProjectId {
        &self.id
    }
}

impl ServiceRequest for ProjectByIdV1 {
    type Response = Option<ItemSummary>;

    const CAPABILITY: CapabilityId = CapabilityId::new("projects.item.by_id", 1);
}

/// Notification emitted after a project has changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectsChangedV1;

impl Event for ProjectsChangedV1 {
    const NAME: &'static str = "projects.changed@1";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_titles_trim_and_validate_input() {
        assert_eq!(
            ProjectTitle::new("  Planning  ").unwrap().as_str(),
            "Planning"
        );
        assert!(ProjectTitle::new(" ").is_err());
        assert!(ProjectTitle::new("bad\ntitle").is_err());
    }

    #[test]
    fn project_ids_require_canonical_uuid_shape() {
        let id = ProjectId::new("550E8400-E29B-41D4-A716-446655440000").unwrap();
        assert_eq!(id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
        assert!(ProjectId::new("not-a-project-id").is_err());
    }

    #[test]
    fn project_lookup_uses_a_versioned_item_capability() {
        let id = ProjectId::new("00000000-0000-4000-8000-000000000001").unwrap();
        let request = ProjectByIdV1::new(id.clone());

        assert_eq!(request.id(), &id);
        assert_eq!(
            ProjectByIdV1::CAPABILITY,
            CapabilityId::new("projects.item.by_id", 1)
        );
    }
}
