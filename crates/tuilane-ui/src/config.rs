use ratatui::style::{Color, Modifier, Style};

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
            Self::Default => Style::default().add_modifier(Modifier::BOLD),
            Self::HighContrast => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        }
    }

    pub(super) fn panel_style(self, focused: bool) -> Style {
        match self {
            Self::Default if focused => Style::default().add_modifier(Modifier::BOLD),
            Self::Default => Style::default(),
            Self::HighContrast if focused => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Self::HighContrast => Style::default().fg(Color::White),
        }
    }
}

/// Initial shell layout preset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LayoutPreset {
    /// Show navigation when terminal width permits it.
    #[default]
    Balanced,
    /// Keep the primary view full-width and navigate views with the keyboard.
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

/// Registered-view navigation key preset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KeymapPreset {
    /// Use the standard shell keys.
    #[default]
    Default,
    /// Also use j/k to move between registered views when a view does not consume them.
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceConfig {
    theme: ThemePreset,
    layout: LayoutPreset,
    keymap: KeymapPreset,
}

impl WorkspaceConfig {
    /// Creates shell configuration from supported built-in presets.
    pub const fn new(theme: ThemePreset, layout: LayoutPreset, keymap: KeymapPreset) -> Self {
        Self {
            theme,
            layout,
            keymap,
        }
    }

    /// Returns the selected shell theme.
    pub const fn theme(self) -> ThemePreset {
        self.theme
    }

    /// Returns the selected layout preset.
    pub const fn layout(self) -> LayoutPreset {
        self.layout
    }

    /// Returns the selected navigation keymap.
    pub const fn keymap(self) -> KeymapPreset {
        self.keymap
    }
}
