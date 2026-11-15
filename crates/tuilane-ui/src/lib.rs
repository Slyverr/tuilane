//! Terminal lifecycle, layout, navigation, and rendering for the tuilane workspace.

mod config;
mod dashboard_view;
pub mod icons;
mod view;
mod workspace;

pub use config::{KeymapPreset, LayoutPreset, ThemePreset, WorkspaceConfig};
pub use dashboard_view::DashboardView;
pub use tuilane_plugin_api::{
    DashboardChoice, DashboardItem, DashboardLayout, DashboardOption, DashboardOptionKind,
    DashboardOptionValue, DashboardSize, DashboardTone, DashboardWidget, DashboardWidgetContent,
    DashboardWidgetDescriptor, DashboardWidgetId, DashboardWidgetPreference,
};
pub use view::{Keybinding, View, ViewAction, ViewDialog, render_empty_state};
pub use workspace::{FocusArea, Workspace, WorkspaceError, require_interactive_terminal, run};
