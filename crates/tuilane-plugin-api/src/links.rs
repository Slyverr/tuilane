//! Public contracts for durable item links and external references.

use std::{error::Error, fmt};

use crate::{
    CapabilityId, Command, CommandPermission, Event, ServiceRequest,
    items::{ItemRef, ItemSummary},
};

/// Kind of an external reference attached to a local item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalReferenceKind {
    File,
    Url,
}

impl ExternalReferenceKind {
    /// Returns the stable storage and display identifier for this reference kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Url => "url",
        }
    }
}

/// Validated file path or HTTP(S) URL attached to a local item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalReference {
    kind: ExternalReferenceKind,
    value: Box<str>,
}

impl ExternalReference {
    /// Creates a local file reference from a nonempty path string.
    pub fn file(value: impl AsRef<str>) -> Result<Self, ExternalReferenceError> {
        Self::new(ExternalReferenceKind::File, value.as_ref())
    }

    /// Creates an HTTP or HTTPS reference with a nonempty authority.
    pub fn url(value: impl AsRef<str>) -> Result<Self, ExternalReferenceError> {
        let value = value.as_ref();
        let valid_scheme = value.split_once("://").is_some_and(|(scheme, authority)| {
            matches!(scheme, "http" | "https")
                && authority
                    .split(['/', '?', '#'])
                    .next()
                    .is_some_and(|host| !host.is_empty())
        });
        if !valid_scheme {
            return Err(ExternalReferenceError);
        }
        Self::new(ExternalReferenceKind::Url, value)
    }

    fn new(kind: ExternalReferenceKind, value: &str) -> Result<Self, ExternalReferenceError> {
        if value.is_empty()
            || value.chars().count() > 2048
            || value.chars().any(|character| {
                character.is_control()
                    || (kind == ExternalReferenceKind::Url && character.is_whitespace())
            })
        {
            return Err(ExternalReferenceError);
        }
        Ok(Self {
            kind,
            value: value.into(),
        })
    }

    /// Returns the reference kind.
    pub const fn kind(&self) -> ExternalReferenceKind {
        self.kind
    }

    /// Returns the path or URL exactly as supplied.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Error returned for an empty, oversized, unsupported, or malformed reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalReferenceError;

impl fmt::Display for ExternalReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "file paths must be nonempty and contain no controls; HTTP(S) URLs must also contain a host and no whitespace",
        )
    }
}

impl Error for ExternalReferenceError {}

/// Directed relation between two stable local items.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemLink {
    source: ItemRef,
    target: ItemRef,
}

impl ItemLink {
    /// Creates a directed link from one item to another.
    pub fn new(source: ItemRef, target: ItemRef) -> Self {
        Self { source, target }
    }

    /// Returns the item that owns this directed link.
    pub fn source(&self) -> &ItemRef {
        &self.source
    }

    /// Returns the linked local item.
    pub fn target(&self) -> &ItemRef {
        &self.target
    }
}

/// Typed operation routed through the host's Links command provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkCommand {
    Add {
        source: ItemRef,
        target: ItemRef,
    },
    Remove {
        source: ItemRef,
        target: ItemRef,
    },
    Outgoing {
        item: ItemRef,
    },
    Backlinks {
        item: ItemRef,
    },
    AddReference {
        owner: ItemRef,
        reference: ExternalReference,
    },
    RemoveReference {
        owner: ItemRef,
        reference: ExternalReference,
    },
    References {
        owner: ItemRef,
    },
}

/// Typed Links command response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkCommandResult {
    Link(ItemLink),
    Removed(ItemLink),
    Items(Vec<ItemSummary>),
    Reference(ExternalReference),
    References(Vec<ExternalReference>),
}

impl Command for LinkCommand {
    type Response = LinkCommandResult;

    const NAME: &'static str = "links";

    fn required_permission(&self) -> CommandPermission {
        match self {
            Self::Outgoing { .. } | Self::Backlinks { .. } | Self::References { .. } => {
                CommandPermission::new("links.read")
            }
            Self::Add { .. } | Self::AddReference { .. } => CommandPermission::new("links.write"),
            Self::Remove { .. } | Self::RemoveReference { .. } => {
                CommandPermission::new("links.delete")
            }
        }
    }
}

/// Notification published after a link or external reference changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinksChangedV1;

impl Event for LinksChangedV1 {
    const NAME: &'static str = "links.changed@1";
}

/// Version 1 request for an item's external references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferencesForItemV1 {
    item: ItemRef,
}

impl ReferencesForItemV1 {
    /// Creates a request for one item's external references.
    pub fn new(item: ItemRef) -> Self {
        Self { item }
    }

    /// Returns the requested item.
    pub fn item(&self) -> &ItemRef {
        &self.item
    }
}

impl ServiceRequest for ReferencesForItemV1 {
    type Response = Vec<ExternalReference>;

    const CAPABILITY: CapabilityId = CapabilityId::new("links.references.for_item", 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_references_validate_supported_values() {
        let url = ExternalReference::url("https://example.org/research").unwrap();
        assert_eq!(url.kind(), ExternalReferenceKind::Url);
        assert_eq!(url.value(), "https://example.org/research");

        let file = ExternalReference::file("notes/research.md").unwrap();
        assert_eq!(file.kind(), ExternalReferenceKind::File);
        assert_eq!(file.value(), "notes/research.md");
        assert!(ExternalReference::file("notes/meeting notes.md").is_ok());

        assert!(ExternalReference::url("ftp://example.org").is_err());
        assert!(ExternalReference::url("https:///path").is_err());
        assert!(ExternalReference::file("bad\npath").is_err());
    }

    #[test]
    fn item_links_preserve_their_direction() {
        let source = ItemRef::new("idea", "idea-id").unwrap();
        let target = ItemRef::new("note", "note-id").unwrap();
        let link = ItemLink::new(source.clone(), target.clone());

        assert_eq!(link.source(), &source);
        assert_eq!(link.target(), &target);
    }

    #[test]
    fn references_service_has_a_versioned_capability() {
        assert_eq!(
            ReferencesForItemV1::CAPABILITY,
            CapabilityId::new("links.references.for_item", 1)
        );
    }
}
