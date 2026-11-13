//! Public task models and command/service contracts.

use std::{error::Error, fmt, str::FromStr};

use crate::{CapabilityId, Command, ServiceRequest};

/// Stable identifier for a task.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskId(Box<str>);

impl TaskId {
    /// Creates a canonical UUID-shaped task identifier.
    pub fn new(value: impl AsRef<str>) -> Result<Self, TaskIdError> {
        let value = value.as_ref();
        let valid = value.len() == 36
            && value.bytes().enumerate().all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            });
        if !valid {
            return Err(TaskIdError);
        }
        Ok(Self(value.to_ascii_lowercase().into()))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for TaskId {
    type Err = TaskIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Error returned for a malformed task identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskIdError;

impl fmt::Display for TaskIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("task identifiers must use the canonical UUID format")
    }
}

impl Error for TaskIdError {}

/// Validated nonempty task title, limited to 200 Unicode scalar values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskTitle(Box<str>);

impl TaskTitle {
    /// Creates a trimmed title and rejects empty, oversized, or control-character input.
    pub fn new(value: impl AsRef<str>) -> Result<Self, TaskTitleError> {
        let value = value.as_ref().trim();
        if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
            return Err(TaskTitleError);
        }
        Ok(Self(value.into()))
    }

    /// Returns the title as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Error returned for a blank, oversized, or unsupported task title.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskTitleError;

impl fmt::Display for TaskTitleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .write_str("task titles must contain 1 to 200 characters and no control characters")
    }
}

impl Error for TaskTitleError {}

/// Validated ISO calendar date in `YYYY-MM-DD` form.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TaskDate(Box<str>);

impl TaskDate {
    /// Parses and validates a Gregorian calendar date in `YYYY-MM-DD` form.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TaskDateError> {
        let value = value.as_ref();
        let bytes = value.as_bytes();
        if bytes.len() != 10
            || !bytes[..4].iter().all(u8::is_ascii_digit)
            || bytes[4] != b'-'
            || !bytes[5..7].iter().all(u8::is_ascii_digit)
            || bytes[7] != b'-'
            || !bytes[8..].iter().all(u8::is_ascii_digit)
        {
            return Err(TaskDateError);
        }

        let year = value[0..4].parse::<u16>().map_err(|_| TaskDateError)?;
        let month = value[5..7].parse::<u8>().map_err(|_| TaskDateError)?;
        let day = value[8..10].parse::<u8>().map_err(|_| TaskDateError)?;
        let leap_year = year % 400 == 0 || (year % 4 == 0 && year % 100 != 0);
        let days_in_month = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap_year => 29,
            2 => 28,
            _ => return Err(TaskDateError),
        };
        if year == 0 || day == 0 || day > days_in_month {
            return Err(TaskDateError);
        }
        Ok(Self(value.into()))
    }

    /// Returns the date as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for TaskDate {
    type Err = TaskDateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Error returned for a malformed or impossible calendar date.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskDateError;

impl fmt::Display for TaskDateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("dates must be valid calendar dates in YYYY-MM-DD format")
    }
}

impl Error for TaskDateError {}

/// Current task state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskStatus {
    Open,
    Completed,
}

impl TaskStatus {
    /// Returns the stable storage representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Task record returned by the host command path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task {
    id: TaskId,
    title: TaskTitle,
    due_date: Option<TaskDate>,
    status: TaskStatus,
    created_at: Box<str>,
    updated_at: Box<str>,
}

impl Task {
    /// Creates a task record from validated fields.
    pub fn new(
        id: TaskId,
        title: TaskTitle,
        due_date: Option<TaskDate>,
        status: TaskStatus,
        created_at: impl Into<Box<str>>,
        updated_at: impl Into<Box<str>>,
    ) -> Self {
        Self {
            id,
            title,
            due_date,
            status,
            created_at: created_at.into(),
            updated_at: updated_at.into(),
        }
    }

    /// Returns the stable task identifier.
    pub fn id(&self) -> &TaskId {
        &self.id
    }

    /// Returns the validated task title.
    pub fn title(&self) -> &TaskTitle {
        &self.title
    }

    /// Returns the optional due date.
    pub fn due_date(&self) -> Option<&TaskDate> {
        self.due_date.as_ref()
    }

    /// Returns whether the task is open or completed.
    pub fn status(&self) -> TaskStatus {
        self.status
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

/// Task summary returned by the date-query service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskSummary {
    id: TaskId,
    title: TaskTitle,
    status: TaskStatus,
}

impl TaskSummary {
    /// Creates a summary from a task record.
    pub fn from_task(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status,
        }
    }

    /// Returns the stable task identifier.
    pub fn id(&self) -> &TaskId {
        &self.id
    }

    /// Returns the task title.
    pub fn title(&self) -> &TaskTitle {
        &self.title
    }

    /// Returns task state.
    pub fn status(&self) -> TaskStatus {
        self.status
    }
}

/// Typed task operation routed through the host command dispatcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskCommand {
    Create {
        title: TaskTitle,
        due_date: Option<TaskDate>,
    },
    Get {
        id: TaskId,
    },
    List {
        include_completed: bool,
    },
    Rename {
        id: TaskId,
        title: TaskTitle,
    },
    SetDueDate {
        id: TaskId,
        due_date: Option<TaskDate>,
    },
    Complete {
        id: TaskId,
    },
    Reopen {
        id: TaskId,
    },
    Delete {
        id: TaskId,
    },
}

/// Typed task command response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskCommandResult {
    Task(Task),
    Tasks(Vec<Task>),
    Deleted(TaskId),
}

impl Command for TaskCommand {
    type Response = TaskCommandResult;

    const NAME: &'static str = "tasks";
}

/// Version 1 query for tasks due on a specific date.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TasksByDateV1 {
    date: TaskDate,
}

impl TasksByDateV1 {
    /// Creates a query for one validated due date.
    pub fn new(date: TaskDate) -> Self {
        Self { date }
    }

    /// Returns the requested date.
    pub fn date(&self) -> &TaskDate {
        &self.date
    }
}

impl ServiceRequest for TasksByDateV1 {
    type Response = Vec<TaskSummary>;

    const CAPABILITY: CapabilityId = CapabilityId::new("tasks.query.by_date", 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_titles_trim_and_validate_input() {
        assert_eq!(
            TaskTitle::new("  Write tests  ").unwrap().as_str(),
            "Write tests"
        );
        assert!(TaskTitle::new(" \t ").is_err());
        assert!(TaskTitle::new("bad\ntitle").is_err());
    }

    #[test]
    fn task_dates_validate_calendar_boundaries() {
        assert!(TaskDate::parse("2024-02-29").is_ok());
        assert!(TaskDate::parse("2023-02-29").is_err());
        assert!(TaskDate::parse("2026-13-01").is_err());
        assert!(TaskDate::parse("2026-2-01").is_err());
    }

    #[test]
    fn task_ids_require_canonical_uuid_shape() {
        let id = TaskId::new("550E8400-E29B-41D4-A716-446655440000").unwrap();
        assert_eq!(id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
        assert!(TaskId::new("not-a-task-id").is_err());
    }
}
