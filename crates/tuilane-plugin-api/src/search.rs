//! Host-mediated full-text search contracts for first-party item features.

use std::{error::Error, fmt};

use crate::{CapabilityId, Command, ServiceRequest, items::SearchDocument};

/// A validated plain-text full-text query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery(Box<str>);

impl SearchQuery {
    /// Creates a trimmed query with a bounded length and no control characters.
    pub fn new(value: impl AsRef<str>) -> Result<Self, SearchQueryError> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(SearchQueryError::Empty);
        }
        if value.chars().count() > 256 {
            return Err(SearchQueryError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(SearchQueryError::ControlCharacter);
        }
        Ok(Self(value.into()))
    }

    /// Returns the normalized query text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned when a search query is empty or invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchQueryError {
    Empty,
    TooLong,
    ControlCharacter,
    InvalidKind,
}

impl fmt::Display for SearchQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("search query cannot be empty"),
            Self::TooLong => formatter.write_str("search query cannot exceed 256 characters"),
            Self::ControlCharacter => {
                formatter.write_str("search query cannot contain control characters")
            }
            Self::InvalidKind => formatter.write_str(
                "search kinds must contain lowercase ASCII letters, digits, '.', '-' or '_'",
            ),
        }
    }
}

impl Error for SearchQueryError {}

/// Search request routed through the host command dispatcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchCommand {
    query: SearchQuery,
    kind: Option<Box<str>>,
}

impl SearchCommand {
    /// Creates an unfiltered search request.
    pub fn new(query: SearchQuery) -> Self {
        Self { query, kind: None }
    }

    /// Restricts results to one validated item kind.
    pub fn with_kind(mut self, kind: impl AsRef<str>) -> Result<Self, SearchQueryError> {
        let kind = kind.as_ref();
        let valid = !kind.is_empty()
            && kind.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            });
        if !valid {
            return Err(SearchQueryError::InvalidKind);
        }
        self.kind = Some(kind.into());
        Ok(self)
    }

    /// Returns the validated search text.
    pub fn query(&self) -> &SearchQuery {
        &self.query
    }

    /// Returns the optional item kind filter.
    pub fn kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }
}

/// Result of searching workspace-owned item text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    item: crate::items::ItemSummary,
    excerpt: Box<str>,
}

impl SearchResult {
    /// Creates a result from validated item metadata and a text excerpt.
    pub fn new(item: crate::items::ItemSummary, excerpt: impl Into<Box<str>>) -> Self {
        Self {
            item,
            excerpt: excerpt.into(),
        }
    }

    /// Returns the matching item's identity and title.
    pub fn item(&self) -> &crate::items::ItemSummary {
        &self.item
    }

    /// Returns a short excerpt around a match.
    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }
}

/// Typed search response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchCommandResult {
    Results(Vec<SearchResult>),
}

impl Command for SearchCommand {
    type Response = SearchCommandResult;

    const NAME: &'static str = "search";
}

/// Requests searchable text owned by Tasks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TaskSearchDocumentsV1;

impl ServiceRequest for TaskSearchDocumentsV1 {
    type Response = Vec<SearchDocument>;

    const CAPABILITY: CapabilityId = CapabilityId::new("tasks.search.documents", 1);
}

/// Requests searchable text owned by Projects.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectSearchDocumentsV1;

impl ServiceRequest for ProjectSearchDocumentsV1 {
    type Response = Vec<SearchDocument>;

    const CAPABILITY: CapabilityId = CapabilityId::new("projects.search.documents", 1);
}

/// Requests searchable text owned by Notes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoteSearchDocumentsV1;

impl ServiceRequest for NoteSearchDocumentsV1 {
    type Response = Vec<SearchDocument>;

    const CAPABILITY: CapabilityId = CapabilityId::new("notes.search.documents", 1);
}

/// Requests searchable text owned by Ideas.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IdeaSearchDocumentsV1;

impl ServiceRequest for IdeaSearchDocumentsV1 {
    type Response = Vec<SearchDocument>;

    const CAPABILITY: CapabilityId = CapabilityId::new("ideas.search.documents", 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_trim_text_and_reject_empty_oversized_or_control_input() {
        assert_eq!(
            SearchQuery::new("  planning  ").unwrap().as_str(),
            "planning"
        );
        assert_eq!(SearchQuery::new("  "), Err(SearchQueryError::Empty));
        assert_eq!(
            SearchQuery::new("x".repeat(257)),
            Err(SearchQueryError::TooLong)
        );
        assert_eq!(
            SearchQuery::new("two\nlines"),
            Err(SearchQueryError::ControlCharacter)
        );
    }

    #[test]
    fn search_kind_filter_accepts_portable_identifiers_only() {
        let command = SearchCommand::new(SearchQuery::new("calendar").unwrap())
            .with_kind("calendar.entry")
            .unwrap();
        assert_eq!(command.kind(), Some("calendar.entry"));
        assert!(
            SearchCommand::new(SearchQuery::new("calendar").unwrap())
                .with_kind("Calendar")
                .is_err()
        );
    }

    #[test]
    fn first_party_search_services_have_distinct_versioned_capabilities() {
        assert_eq!(
            TaskSearchDocumentsV1::CAPABILITY,
            CapabilityId::new("tasks.search.documents", 1)
        );
        assert_eq!(
            ProjectSearchDocumentsV1::CAPABILITY,
            CapabilityId::new("projects.search.documents", 1)
        );
        assert_eq!(
            NoteSearchDocumentsV1::CAPABILITY,
            CapabilityId::new("notes.search.documents", 1)
        );
        assert_eq!(
            IdeaSearchDocumentsV1::CAPABILITY,
            CapabilityId::new("ideas.search.documents", 1)
        );
    }
}
