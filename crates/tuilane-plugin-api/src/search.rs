//! Host-mediated full-text search contracts for first-party item features.

use std::{error::Error, fmt};

use crate::{CapabilityId, Command, CommandPermission, ServiceRequest, items::SearchDocument};

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

/// A validated user-facing name for a saved search view.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SavedSearchName(Box<str>);

impl SavedSearchName {
    /// Creates a trimmed name containing between 1 and 64 non-control characters.
    pub fn new(value: impl AsRef<str>) -> Result<Self, SavedSearchNameError> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(SavedSearchNameError::Empty);
        }
        if value.chars().count() > 64 {
            return Err(SavedSearchNameError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(SavedSearchNameError::ControlCharacter);
        }
        Ok(Self(value.into()))
    }

    /// Returns the normalized saved search name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned when a saved search name is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavedSearchNameError {
    Empty,
    TooLong,
    ControlCharacter,
}

impl fmt::Display for SavedSearchNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("saved search name cannot be empty"),
            Self::TooLong => formatter.write_str("saved search name cannot exceed 64 characters"),
            Self::ControlCharacter => {
                formatter.write_str("saved search name cannot contain control characters")
            }
        }
    }
}

impl Error for SavedSearchNameError {}

/// A named search query with an optional item-kind filter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedSearch {
    name: SavedSearchName,
    query: SearchQuery,
    kind: Option<Box<str>>,
}

impl SavedSearch {
    /// Creates a saved search and validates its optional item-kind filter.
    pub fn new(
        name: SavedSearchName,
        query: SearchQuery,
        kind: Option<&str>,
    ) -> Result<Self, SearchQueryError> {
        let command = SearchCommand::new(query.clone());
        let command = match kind {
            Some(kind) => command.with_kind(kind)?,
            None => command,
        };
        Ok(Self {
            name,
            query,
            kind: command.kind().map(Into::into),
        })
    }

    /// Returns the saved view's name.
    pub fn name(&self) -> &SavedSearchName {
        &self.name
    }

    /// Returns the saved query text.
    pub fn query(&self) -> &SearchQuery {
        &self.query
    }

    /// Returns the optional item-kind filter.
    pub fn kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }

    /// Reconstructs the validated command represented by this saved view.
    pub fn command(&self) -> Result<SearchCommand, SearchQueryError> {
        let command = SearchCommand::new(self.query.clone());
        match self.kind() {
            Some(kind) => command.with_kind(kind),
            None => Ok(command),
        }
    }
}

/// Saves one named search view without replacing an existing name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveSavedSearchCommand(SavedSearch);

impl SaveSavedSearchCommand {
    /// Creates a save request for a validated search view.
    pub fn new(search: SavedSearch) -> Self {
        Self(search)
    }

    /// Returns the saved search to persist.
    pub fn search(&self) -> &SavedSearch {
        &self.0
    }
}

impl Command for SaveSavedSearchCommand {
    type Response = SavedSearch;

    const NAME: &'static str = "search.saved.save";

    fn required_permission(&self) -> CommandPermission {
        CommandPermission::new("search.saved.write")
    }
}

/// Lists saved search views in stable name order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ListSavedSearchesCommand;

impl Command for ListSavedSearchesCommand {
    type Response = Vec<SavedSearch>;

    const NAME: &'static str = "search.saved.list";

    fn required_permission(&self) -> CommandPermission {
        CommandPermission::new("search.saved.read")
    }
}

/// Deletes one saved search view by name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteSavedSearchCommand(SavedSearchName);

impl DeleteSavedSearchCommand {
    /// Creates a deletion request for a validated saved search name.
    pub fn new(name: SavedSearchName) -> Self {
        Self(name)
    }

    /// Returns the saved search name to delete.
    pub fn name(&self) -> &SavedSearchName {
        &self.0
    }
}

impl Command for DeleteSavedSearchCommand {
    type Response = bool;

    const NAME: &'static str = "search.saved.delete";

    fn required_permission(&self) -> CommandPermission {
        CommandPermission::new("search.saved.delete")
    }
}

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

    fn required_permission(&self) -> CommandPermission {
        CommandPermission::new("search.read")
    }
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
    fn saved_search_names_are_trimmed_and_bounded() {
        assert_eq!(
            SavedSearchName::new("  Weekly review  ").unwrap().as_str(),
            "Weekly review"
        );
        assert_eq!(SavedSearchName::new("  "), Err(SavedSearchNameError::Empty));
        assert_eq!(
            SavedSearchName::new("x".repeat(65)),
            Err(SavedSearchNameError::TooLong)
        );
        assert_eq!(
            SavedSearchName::new("two\nlines"),
            Err(SavedSearchNameError::ControlCharacter)
        );
    }

    #[test]
    fn saved_search_preserves_its_query_and_validated_kind() {
        let saved = SavedSearch::new(
            SavedSearchName::new("Open tasks").unwrap(),
            SearchQuery::new("  review  ").unwrap(),
            Some("task"),
        )
        .unwrap();

        assert_eq!(saved.query().as_str(), "review");
        assert_eq!(saved.kind(), Some("task"));
        assert_eq!(saved.command().unwrap().kind(), Some("task"));
        assert_eq!(
            SavedSearch::new(
                SavedSearchName::new("Invalid").unwrap(),
                SearchQuery::new("review").unwrap(),
                Some("Task"),
            ),
            Err(SearchQueryError::InvalidKind)
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
