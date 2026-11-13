//! Stable cross-feature item references and notifications.

use std::{error::Error, fmt, str::FromStr};

use crate::Event;

/// Stable reference to an item owned by a feature plugin.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemRef {
    kind: Box<str>,
    id: Box<str>,
}

impl ItemRef {
    /// Creates a reference using a lowercase kind and portable identifier.
    pub fn new(kind: impl AsRef<str>, id: impl AsRef<str>) -> Result<Self, ItemReferenceError> {
        let kind = kind.as_ref();
        let id = id.as_ref();
        let valid_kind = !kind.is_empty()
            && kind.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            });
        if !valid_kind {
            return Err(ItemReferenceError::InvalidKind);
        }

        let valid_id = !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
        if !valid_id {
            return Err(ItemReferenceError::InvalidId);
        }

        Ok(Self {
            kind: kind.into(),
            id: id.into(),
        })
    }

    /// Returns the owning feature kind, such as `task` or `project`.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the stable identifier within the owning feature.
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for ItemRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.kind, self.id)
    }
}

impl FromStr for ItemRef {
    type Err = ItemReferenceError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((kind, id)) = value.split_once(':') else {
            return Err(ItemReferenceError::InvalidFormat);
        };
        if id.contains(':') {
            return Err(ItemReferenceError::InvalidFormat);
        }
        Self::new(kind, id)
    }
}

/// Display metadata returned when a feature resolves an item reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemSummary {
    reference: ItemRef,
    title: Box<str>,
}

/// Text supplied by an item owner for host-mediated full-text indexing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchDocument {
    item: ItemSummary,
    content: Box<str>,
}

impl SearchDocument {
    /// Creates a search document using validated item identity and title.
    pub fn new(item: ItemSummary, content: impl Into<Box<str>>) -> Self {
        Self {
            item,
            content: content.into(),
        }
    }

    /// Returns the stable item identity and title.
    pub fn item(&self) -> &ItemSummary {
        &self.item
    }

    /// Returns text owned by the feature and approved for indexing.
    pub fn content(&self) -> &str {
        &self.content
    }
}

impl ItemSummary {
    /// Creates display metadata for a resolved item.
    pub fn new(reference: ItemRef, title: impl AsRef<str>) -> Result<Self, ItemSummaryError> {
        let title = title.as_ref().trim();
        if title.is_empty() || title.chars().count() > 200 || title.chars().any(char::is_control) {
            return Err(ItemSummaryError);
        }
        Ok(Self {
            reference,
            title: title.into(),
        })
    }

    /// Returns the resolved item's stable reference.
    pub fn reference(&self) -> &ItemRef {
        &self.reference
    }

    /// Returns the item's user-facing title.
    pub fn title(&self) -> &str {
        &self.title
    }
}

/// Notification published after a feature removes an item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemRemovedV1 {
    item: ItemRef,
}

impl ItemRemovedV1 {
    /// Creates a removal notification for one stable item reference.
    pub fn new(item: ItemRef) -> Self {
        Self { item }
    }

    /// Returns the item that was removed.
    pub fn item(&self) -> &ItemRef {
        &self.item
    }
}

impl Event for ItemRemovedV1 {
    const NAME: &'static str = "items.removed@1";
}

/// Error returned when an item reference has an invalid kind, ID, or encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemReferenceError {
    InvalidKind,
    InvalidId,
    InvalidFormat,
}

impl fmt::Display for ItemReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKind => formatter.write_str(
                "item kinds must contain lowercase ASCII letters, digits, '.', '-' or '_'",
            ),
            Self::InvalidId => formatter
                .write_str("item IDs must contain 1 to 128 ASCII letters, digits, '.', '-' or '_'"),
            Self::InvalidFormat => formatter.write_str("item references must use kind:id format"),
        }
    }
}

impl Error for ItemReferenceError {}

/// Error returned when an item summary has an invalid display title.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemSummaryError;

impl fmt::Display for ItemSummaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("item titles must contain 1 to 200 characters and no controls")
    }
}

impl Error for ItemSummaryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_references_roundtrip_through_their_display_format() {
        let item = ItemRef::new("project", "a12b-34").expect("valid item reference");

        assert_eq!(item.to_string().parse::<ItemRef>(), Ok(item));
    }

    #[test]
    fn item_references_reject_invalid_kinds_and_ids() {
        assert_eq!(
            ItemRef::new("Task", "a12b"),
            Err(ItemReferenceError::InvalidKind)
        );
        assert_eq!(
            ItemRef::new("task", "bad id"),
            Err(ItemReferenceError::InvalidId)
        );
    }

    #[test]
    fn item_reference_parser_rejects_ambiguous_separators() {
        assert_eq!(
            "task:abc:def".parse::<ItemRef>(),
            Err(ItemReferenceError::InvalidFormat)
        );
    }

    #[test]
    fn removal_notifications_preserve_the_item_reference() {
        let item = ItemRef::new("note", "note-1").expect("valid item reference");

        assert_eq!(ItemRemovedV1::new(item.clone()).item(), &item);
    }

    #[test]
    fn item_summaries_reject_empty_titles() {
        let item = ItemRef::new("project", "project-1").expect("valid item reference");

        assert_eq!(ItemSummary::new(item, "  "), Err(ItemSummaryError));
    }
}
