use ratatui::{Frame, layout::Rect};
use tuilane_plugin_api::{ViewDescriptor, ViewId};

use crate::{FocusArea, Keybinding, View, icons, render_empty_state};

/// Host-owned entry point for the workspace's plugin-contributed Overview.
pub struct DashboardView {
    descriptor: ViewDescriptor,
}

impl DashboardView {
    /// Creates the Overview tab rendered by the workspace dashboard host.
    pub fn new() -> Self {
        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("overview").expect("static view identifier is valid"),
                "Overview",
                "Your configurable workspace dashboard",
            ),
        }
    }
}

impl Default for DashboardView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for DashboardView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn icon(&self) -> &'static str {
        icons::OVERVIEW
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        render_empty_state(
            frame,
            area,
            "Your overview is empty",
            "Add widgets from the available views to build your workspace.",
        );
    }

    fn is_dashboard(&self) -> bool {
        true
    }

    fn keybindings(&self, focus: FocusArea) -> Vec<Keybinding> {
        if focus != FocusArea::Content {
            return Vec::new();
        }

        vec![
            Keybinding::new("Open", "Enter").with_priority(90),
            Keybinding::new("Select", "↑/↓, j/k").with_priority(10),
            Keybinding::new("Customize", "e").with_priority(100),
        ]
    }
}
