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

    /// Returns the heading style used by the selected theme.
    pub fn header_style(self) -> Style {
        match self {
            Self::Default => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Self::HighContrast => Self::accent_style(self).add_modifier(Modifier::BOLD),
        }
    }

    /// Returns the accent style used by the selected theme.
    pub fn accent_style(self) -> Style {
        match self {
            Self::Default => Style::default().fg(Color::Cyan),
            Self::HighContrast => Style::default().fg(Color::Yellow),
        }
    }

    /// Returns the secondary-text style used by the selected theme.
    pub fn muted_style(self) -> Style {
        match self {
            Self::Default => Style::default().fg(Color::Gray),
            Self::HighContrast => Style::default().fg(Color::White),
        }
    }

    /// Returns the color style for successful or completed states.
    pub fn positive_style(self) -> Style {
        match self {
            Self::Default => Style::default().fg(Color::Green),
            Self::HighContrast => Style::default().fg(Color::LightGreen),
        }
    }

    /// Returns the color style for states that need attention.
    pub fn warning_style(self) -> Style {
        match self {
            Self::Default => Style::default().fg(Color::Yellow),
            Self::HighContrast => Style::default().fg(Color::LightYellow),
        }
    }

    /// Returns the color style for failed or critical states.
    pub fn critical_style(self) -> Style {
        match self {
            Self::Default => Style::default().fg(Color::Red),
            Self::HighContrast => Style::default().fg(Color::LightRed),
        }
    }

    /// Returns the foreground style for a focused or inactive panel.
    pub fn panel_style(self, focused: bool) -> Style {
        match self {
            Self::Default if focused => Self::accent_style(self).add_modifier(Modifier::BOLD),
            Self::Default => Style::default(),
            Self::HighContrast if focused => Self::accent_style(self).add_modifier(Modifier::BOLD),
            Self::HighContrast => Style::default().fg(Color::White),
        }
    }

    /// Returns the foreground and selection background for a navigation row.
    pub fn navigation_item_style(self, selected: bool) -> Style {
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

/// Selects the shell's view-navigation presentation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NavigationStyle {
    /// Place views in a horizontal bar above the workspace.
    #[default]
    Tabs,
    /// Place views in a vertical rail beside the workspace.
    Sidebar,
}

impl NavigationStyle {
    /// Returns the configuration name for this navigation style.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tabs => "tabs",
            Self::Sidebar => "sidebar",
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
    navigation: NavigationStyle,
    navigation_collapsed: bool,
    dashboard: DashboardLayout,
}

impl WorkspaceConfig {
    /// Creates shell configuration from supported built-in presets.
    pub fn new(theme: ThemePreset, layout: LayoutPreset, keymap: KeymapPreset) -> Self {
        Self {
            theme,
            layout,
            keymap,
            navigation: NavigationStyle::default(),
            navigation_collapsed: false,
            dashboard: DashboardLayout::default(),
        }
    }

    /// Sets the navigation presentation and whether it shows labels.
    pub fn with_navigation(mut self, navigation: NavigationStyle, collapsed: bool) -> Self {
        self.navigation = navigation;
        self.navigation_collapsed = collapsed;
        self
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

    /// Returns the selected navigation presentation.
    pub const fn navigation(&self) -> NavigationStyle {
        self.navigation
    }

    /// Returns whether navigation labels are collapsed to icons.
    pub const fn navigation_collapsed(&self) -> bool {
        self.navigation_collapsed
    }

    /// Changes the navigation presentation.
    pub fn set_navigation(&mut self, navigation: NavigationStyle) {
        self.navigation = navigation;
    }

    /// Shows or hides navigation labels.
    pub fn set_navigation_collapsed(&mut self, collapsed: bool) {
        self.navigation_collapsed = collapsed;
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
