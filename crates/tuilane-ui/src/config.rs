use ratatui::style::{Color, Modifier, Style};
use tuilane_plugin_api::DashboardLayout;

/// Built-in shell accent themes supported by the terminal interface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThemePreset {
    /// Use the terminal's default foreground and background colors.
    #[default]
    Default,
    /// Use bright text and yellow focus accents for stronger contrast.
    HighContrast,
}

impl ThemePreset {
    /// Returns the configuration name for this preset.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::HighContrast => "high-contrast",
        }
    }

    pub(super) fn header_style(self) -> Style {
        match self {
            Self::Default => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Self::HighContrast => Self::accent_style(self).add_modifier(Modifier::BOLD),
        }
    }

    pub(super) fn accent_style(self) -> Style {
        match self {
            Self::Default => Style::default().fg(Color::Cyan),
            Self::HighContrast => Style::default().fg(Color::Yellow),
        }
    }

    pub(super) fn muted_style(self) -> Style {
        match self {
            Self::Default => Style::default().fg(Color::Gray),
            Self::HighContrast => Style::default().fg(Color::White),
        }
    }

    pub(super) fn panel_style(self, focused: bool) -> Style {
        match self {
            Self::Default if focused => Self::accent_style(self).add_modifier(Modifier::BOLD),
            Self::Default => Style::default(),
            Self::HighContrast if focused => Self::accent_style(self).add_modifier(Modifier::BOLD),
            Self::HighContrast => Style::default().fg(Color::White),
        }
    }

    pub(super) fn navigation_item_style(self, selected: bool) -> Style {
        if !selected {
            return Style::default();
        }

        let background = match self {
            Self::Default => Color::Cyan,
            Self::HighContrast => Color::Yellow,
        };
        Style::default()
            .fg(Color::Black)
            .bg(background)
            .add_modifier(Modifier::BOLD)
    }
}

/// Initial shell layout preset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LayoutPreset {
    /// Show the contextual side panel when terminal width permits it.
    #[default]
    Balanced,
    /// Keep the primary view full-width by hiding the contextual side panel.
    Focus,
}

impl LayoutPreset {
    /// Returns the configuration name for this preset.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Focus => "focus",
        }
    }
}

/// Configured view-navigation key preset name.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KeymapPreset {
    /// Use the standard shell key preset.
    #[default]
    Default,
    /// Keep accepting the Vim preset name from existing configuration files.
    Vim,
}

impl KeymapPreset {
    /// Returns the configuration name for this preset.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Vim => "vim",
        }
    }
}

/// Supported presentation choices for the host-owned workspace shell.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceConfig {
    theme: ThemePreset,
    layout: LayoutPreset,
    keymap: KeymapPreset,
    dashboard: DashboardLayout,
}

impl WorkspaceConfig {
    /// Creates shell configuration from supported built-in presets.
    pub fn new(theme: ThemePreset, layout: LayoutPreset, keymap: KeymapPreset) -> Self {
        Self {
            theme,
            layout,
            keymap,
            dashboard: DashboardLayout::default(),
        }
    }

    /// Uses a saved dashboard layout with the selected shell presets.
    pub fn with_dashboard_layout(mut self, dashboard: DashboardLayout) -> Self {
        self.dashboard = dashboard;
        self
    }

    /// Returns the selected shell theme.
    pub const fn theme(&self) -> ThemePreset {
        self.theme
    }

    /// Returns the selected layout preset.
    pub const fn layout(&self) -> LayoutPreset {
        self.layout
    }

    /// Returns the selected navigation keymap.
    pub const fn keymap(&self) -> KeymapPreset {
        self.keymap
    }

    /// Returns the user's saved dashboard widget order and settings.
    pub fn dashboard_layout(&self) -> &DashboardLayout {
        &self.dashboard
    }

    /// Returns mutable access to the dashboard layout edited by the host UI.
    pub fn dashboard_layout_mut(&mut self) -> &mut DashboardLayout {
        &mut self.dashboard
    }
}
