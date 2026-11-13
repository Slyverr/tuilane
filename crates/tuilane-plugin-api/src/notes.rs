//! Public note models and command/service contracts.

use std::{error::Error, fmt, str::FromStr};

use crate::{CapabilityId, Command, Event, ServiceRequest, items::ItemSummary};

/// Stable identifier for a note.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NoteId(Box<str>);

impl NoteId {
    /// Creates a canonical UUID-shaped note identifier.
    pub fn new(value: impl AsRef<str>) -> Result<Self, NoteIdError> {
        let value = value.as_ref();
        let valid = value.len() == 36
            && value.bytes().enumerate().all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            });
        if !valid {
            return Err(NoteIdError);
        }
        Ok(Self(value.to_ascii_lowercase().into()))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NoteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for NoteId {
    type Err = NoteIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Error returned for a malformed note identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoteIdError;

impl fmt::Display for NoteIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("note identifiers must use the canonical UUID format")
    }
}

impl Error for NoteIdError {}

/// Validated nonempty note title, limited to 200 Unicode scalar values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteTitle(Box<str>);

impl NoteTitle {
    /// Creates a trimmed title and rejects empty, oversized, or control-character input.
    pub fn new(value: impl AsRef<str>) -> Result<Self, NoteTitleError> {
        let value = value.as_ref().trim();
        if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
            return Err(NoteTitleError);
        }
        Ok(Self(value.into()))
    }

    /// Returns the validated title.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NoteTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Error returned for an invalid note title.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoteTitleError;

impl fmt::Display for NoteTitleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("note titles must contain 1 to 200 characters and no controls")
    }
}

impl Error for NoteTitleError {}

/// Markdown content with a one-megabyte UTF-8 size limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Markdown(Box<str>);

impl Markdown {
    /// Creates Markdown content, rejecting NUL bytes and oversized input.
    pub fn new(value: impl AsRef<str>) -> Result<Self, MarkdownError> {
        let value = value.as_ref();
        if value.len() > 1_048_576 || value.contains('\0') {
            return Err(MarkdownError);
        }
        Ok(Self(value.into()))
    }

    /// Returns the Markdown source.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned for oversized or invalid Markdown content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkdownError;

impl fmt::Display for MarkdownError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Markdown content must be at most 1 MiB and contain no NUL bytes")
    }
}

impl Error for MarkdownError {}

/// Durable Markdown note returned by the Notes feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Note {
    id: NoteId,
    title: NoteTitle,
    body: Markdown,
    created_at: Box<str>,
    updated_at: Box<str>,
}

impl Note {
    /// Creates a note record from validated fields.
    pub fn new(
        id: NoteId,
        title: NoteTitle,
        body: Markdown,
        created_at: impl Into<Box<str>>,
        updated_at: impl Into<Box<str>>,
    ) -> Self {
        Self {
            id,
            title,
            body,
            created_at: created_at.into(),
            updated_at: updated_at.into(),
        }
    }

    /// Returns the stable note identifier.
    pub fn id(&self) -> &NoteId {
        &self.id
    }

    /// Returns the validated note title.
    pub fn title(&self) -> &NoteTitle {
        &self.title
    }

    /// Returns the Markdown source.
    pub fn body(&self) -> &Markdown {
        &self.body
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

/// Typed note operation routed through the host command dispatcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NoteCommand {
    Create { title: NoteTitle, body: Markdown },
    Get { id: NoteId },
    List,
    Rename { id: NoteId, title: NoteTitle },
    SetBody { id: NoteId, body: Markdown },
    Delete { id: NoteId },
}

/// Typed note command response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NoteCommandResult {
    Note(Note),
    Notes(Vec<Note>),
    Deleted(NoteId),
}

impl Command for NoteCommand {
    type Response = NoteCommandResult;

    const NAME: &'static str = "notes";
}

/// Version 1 lookup for a note's stable cross-feature item reference and title.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteByIdV1 {
    id: NoteId,
}

impl NoteByIdV1 {
    /// Creates a lookup for one validated note ID.
    pub fn new(id: NoteId) -> Self {
        Self { id }
    }

    /// Returns the requested note ID.
    pub fn id(&self) -> &NoteId {
        &self.id
    }
}

impl ServiceRequest for NoteByIdV1 {
    type Response = Option<ItemSummary>;

    const CAPABILITY: CapabilityId = CapabilityId::new("notes.item.by_id", 1);
}

/// Notification emitted after a note has changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotesChangedV1;

impl Event for NotesChangedV1 {
    const NAME: &'static str = "notes.changed@1";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_titles_trim_and_validate_input() {
        assert_eq!(NoteTitle::new("  Research  ").unwrap().as_str(), "Research");
        assert!(NoteTitle::new(" ").is_err());
        assert!(NoteTitle::new("bad\ntitle").is_err());
    }

    #[test]
    fn markdown_accepts_normal_text_and_rejects_nul_and_oversize_content() {
        assert_eq!(
            Markdown::new("# Heading\n\nBody").unwrap().as_str(),
            "# Heading\n\nBody"
        );
        assert!(Markdown::new("bad\0markdown").is_err());
        assert!(Markdown::new("x".repeat(1_048_577)).is_err());
    }

    #[test]
    fn note_ids_require_canonical_uuid_shape() {
        let id = NoteId::new("550E8400-E29B-41D4-A716-446655440000").unwrap();
        assert_eq!(id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
        assert!(NoteId::new("not-a-note-id").is_err());
    }

    #[test]
    fn note_lookup_uses_a_versioned_item_capability() {
        let id = NoteId::new("00000000-0000-4000-8000-000000000001").unwrap();
        let request = NoteByIdV1::new(id.clone());

        assert_eq!(request.id(), &id);
        assert_eq!(
            NoteByIdV1::CAPABILITY,
            CapabilityId::new("notes.item.by_id", 1)
        );
    }
}
