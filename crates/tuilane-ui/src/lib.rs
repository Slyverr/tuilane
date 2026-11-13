//! Terminal lifecycle, layout, navigation, and rendering for the tuilane workspace.

mod view;
mod workspace;

pub use view::{View, ViewAction};
pub use workspace::{FocusArea, Workspace, WorkspaceError, require_interactive_terminal, run};
