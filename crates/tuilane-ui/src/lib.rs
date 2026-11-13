//! Terminal lifecycle, layout, navigation, and rendering for the tuilane workspace.

mod config;
mod view;
mod workspace;

pub use config::{KeymapPreset, LayoutPreset, ThemePreset, WorkspaceConfig};
pub use view::{View, ViewAction};
pub use workspace::{FocusArea, Workspace, WorkspaceError, require_interactive_terminal, run};
