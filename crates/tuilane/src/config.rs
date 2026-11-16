use std::{
    env,
    error::Error,
    fmt, fs, io,
    io::Write,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use directories::ProjectDirs;
use serde::Deserialize;
use tuilane_assistant::{AssistantConfig, AssistantConfigError, DEFAULT_ENDPOINT};
use tuilane_ui::{
    DashboardLayout, KeymapPreset, LayoutPreset, NavigationStyle, ThemePreset, WorkspaceConfig,
};

const CONFIG_ENV: &str = "TUILANE_CONFIG";
const CONFIG_VERSION: u32 = 1;

#[derive(Debug)]
pub enum ConfigError {
    ConfigDirectoryUnavailable,
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Write {
        path: PathBuf,
        source: io::Error,
    },
    DashboardSerialize {
        path: PathBuf,
        source: toml_edit::ser::Error,
    },
    DocumentParse {
        path: PathBuf,
        source: toml_edit::TomlError,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    UnsupportedVersion {
        version: u32,
        path: PathBuf,
    },
    UnsupportedValue {
        key: &'static str,
        value: String,
        path: PathBuf,
    },
    MissingValue {
        key: &'static str,
        path: PathBuf,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigDirectoryUnavailable => {
                formatter.write_str("could not determine the application configuration directory")
            }
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "could not read configuration {}: {source}",
                    path.display()
                )
            }
            Self::Write { path, source } => write!(
                formatter,
                "could not save configuration {}: {source}",
                path.display()
            ),
            Self::DashboardSerialize { path, source } => write!(
                formatter,
                "could not encode dashboard configuration {}: {source}",
                path.display()
            ),
            Self::DocumentParse { path, source } => write!(
                formatter,
                "invalid configuration document {}: {source}",
                path.display()
            ),
            Self::Parse { path, source } => {
                write!(
                    formatter,
                    "invalid configuration {}: {source}",
                    path.display()
                )
            }
            Self::UnsupportedVersion { version, path } => write!(
                formatter,
                "invalid configuration {}: version {version} is unsupported; expected {CONFIG_VERSION}",
                path.display()
            ),
            Self::UnsupportedValue { key, value, path } => write!(
                formatter,
                "invalid configuration {}: unsupported value '{value}' for {key}",
                path.display()
            ),
            Self::MissingValue { key, path } => {
                write!(
                    formatter,
                    "invalid configuration {}: missing {key}",
                    path.display()
                )
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Write { source, .. } => Some(source),
            Self::DashboardSerialize { source, .. } => Some(source),
            Self::DocumentParse { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::ConfigDirectoryUnavailable
            | Self::UnsupportedVersion { .. }
            | Self::UnsupportedValue { .. }
            | Self::MissingValue { .. } => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    version: u32,
    ui: UiConfig,
    assistant: AssistantFileConfig,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            ui: UiConfig::default(),
            assistant: AssistantFileConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AssistantFileConfig {
    enabled: bool,
    model: String,
    endpoint: String,
}

impl Default for AssistantFileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: String::new(),
            endpoint: DEFAULT_ENDPOINT.to_owned(),
        }
    }
}

pub(super) struct ApplicationConfig {
    pub(super) workspace: WorkspaceConfig,
    pub(super) assistant: Option<AssistantConfig>,
    pub(super) path: PathBuf,
    pub(super) original_workspace: WorkspaceConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiConfig {
    theme: String,
    layout: String,
    keymap: String,
    navigation: String,
    navigation_collapsed: bool,
    dashboard: DashboardLayout,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: ThemePreset::default().as_str().to_owned(),
            layout: LayoutPreset::default().as_str().to_owned(),
            keymap: KeymapPreset::default().as_str().to_owned(),
            navigation: NavigationStyle::default().as_str().to_owned(),
            navigation_collapsed: false,
            dashboard: DashboardLayout::default(),
        }
    }
}

impl FileConfig {
    fn workspace(&self, path: &Path) -> Result<WorkspaceConfig, ConfigError> {
        if self.version != CONFIG_VERSION {
            return Err(ConfigError::UnsupportedVersion {
                version: self.version,
                path: path.to_owned(),
            });
        }
        let theme = match self.ui.theme.as_str() {
            "default" => ThemePreset::Default,
            "high-contrast" => ThemePreset::HighContrast,
            _ => {
                return Err(ConfigError::UnsupportedValue {
                    key: "ui.theme",
                    value: self.ui.theme.clone(),
                    path: path.to_owned(),
                });
            }
        };
        let layout = match self.ui.layout.as_str() {
            "balanced" => LayoutPreset::Balanced,
            "focus" => LayoutPreset::Focus,
            _ => {
                return Err(ConfigError::UnsupportedValue {
                    key: "ui.layout",
                    value: self.ui.layout.clone(),
                    path: path.to_owned(),
                });
            }
        };
        let keymap = match self.ui.keymap.as_str() {
            "default" => KeymapPreset::Default,
            "vim" => KeymapPreset::Vim,
            _ => {
                return Err(ConfigError::UnsupportedValue {
                    key: "ui.keymap",
                    value: self.ui.keymap.clone(),
                    path: path.to_owned(),
                });
            }
        };
        let navigation = match self.ui.navigation.as_str() {
            "tabs" => NavigationStyle::Tabs,
            "sidebar" => NavigationStyle::Sidebar,
            _ => {
                return Err(ConfigError::UnsupportedValue {
                    key: "ui.navigation",
                    value: self.ui.navigation.clone(),
                    path: path.to_owned(),
                });
            }
        };
        Ok(WorkspaceConfig::new(theme, layout, keymap)
            .with_navigation(navigation, self.ui.navigation_collapsed)
            .with_dashboard_layout(self.ui.dashboard.clone()))
    }

    fn application(&self, path: &Path) -> Result<ApplicationConfig, ConfigError> {
        let workspace = self.workspace(path)?;
        AssistantConfig::validate_endpoint(&self.assistant.endpoint).map_err(|error| {
            assistant_config_error(error, "assistant.endpoint", &self.assistant.endpoint, path)
        })?;
        let assistant = if self.assistant.enabled {
            if self.assistant.model.trim().is_empty() {
                return Err(ConfigError::MissingValue {
                    key: "assistant.model",
                    path: path.to_owned(),
                });
            }
            Some(
                AssistantConfig::new(&self.assistant.model, &self.assistant.endpoint).map_err(
                    |error| {
                        assistant_config_error(
                            error,
                            "assistant.model",
                            &self.assistant.model,
                            path,
                        )
                    },
                )?,
            )
        } else {
            None
        };
        Ok(ApplicationConfig {
            original_workspace: workspace.clone(),
            workspace,
            assistant,
            path: path.to_owned(),
        })
    }
}

fn assistant_config_error(
    error: AssistantConfigError,
    key: &'static str,
    value: &str,
    path: &Path,
) -> ConfigError {
    ConfigError::UnsupportedValue {
        key,
        value: format!("{value} ({error})"),
        path: path.to_owned(),
    }
}

pub(super) fn load_application_config() -> Result<ApplicationConfig, ConfigError> {
    let (path, explicit) = resolve_config_path(None)?;
    match fs::read_to_string(&path) {
        Ok(contents) => parse_application_config(&contents, &path),
        Err(error) if error.kind() == io::ErrorKind::NotFound && !explicit => {
            let workspace = WorkspaceConfig::default();
            Ok(ApplicationConfig {
                original_workspace: workspace.clone(),
                workspace,
                assistant: None,
                path,
            })
        }
        Err(source) => Err(ConfigError::Read { path, source }),
    }
}

pub(super) fn save_workspace_preferences(
    path: &Path,
    original: &WorkspaceConfig,
    workspace: &WorkspaceConfig,
) -> Result<(), ConfigError> {
    let dashboard_changed = original.dashboard_layout() != workspace.dashboard_layout();
    let navigation_changed = original.navigation() != workspace.navigation();
    let collapse_changed = original.navigation_collapsed() != workspace.navigation_collapsed();
    if !dashboard_changed && !navigation_changed && !collapse_changed {
        return Ok(());
    }

    let contents = match fs::read_to_string(path) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            });
        }
    };
    if let Some(contents) = &contents {
        let file_config =
            toml::from_str::<FileConfig>(contents).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            })?;
        file_config.workspace(path)?;
    }

    let mut document = contents
        .as_deref()
        .unwrap_or_default()
        .parse::<toml_edit::DocumentMut>()
        .map_err(|source| ConfigError::DocumentParse {
            path: path.to_owned(),
            source,
        })?;
    let ui = document
        .as_table_mut()
        .entry("ui")
        .or_insert(toml_edit::table());
    let Some(ui) = ui.as_table_mut() else {
        return Err(ConfigError::UnsupportedValue {
            key: "ui",
            value: "expected a table".to_owned(),
            path: path.to_owned(),
        });
    };
    if dashboard_changed {
        let serialized_dashboard = toml_edit::ser::to_document(workspace.dashboard_layout())
            .map_err(|source| ConfigError::DashboardSerialize {
                path: path.to_owned(),
                source,
            })?;
        ui.insert(
            "dashboard",
            toml_edit::Item::Table(serialized_dashboard.into_table()),
        );
    }
    if navigation_changed {
        ui.insert(
            "navigation",
            toml_edit::value(workspace.navigation().as_str()),
        );
    }
    if collapse_changed {
        ui.insert(
            "navigation_collapsed",
            toml_edit::value(workspace.navigation_collapsed()),
        );
    }
    let contents = document.to_string();
    write_config_atomically(path, contents.as_bytes())
}

fn write_config_atomically(path: &Path, contents: &[u8]) -> Result<(), ConfigError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: path.to_owned(),
            source,
        })?;
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tuilane.toml");
    let temporary_path =
        path.with_file_name(format!(".{file_name}.{}.{}.tmp", process::id(), nonce));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut temporary = options
        .open(&temporary_path)
        .map_err(|source| ConfigError::Write {
            path: path.to_owned(),
            source,
        })?;

    let write_result = temporary
        .write_all(contents)
        .and_then(|()| temporary.sync_all());
    drop(temporary);
    if let Err(source) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(ConfigError::Write {
            path: path.to_owned(),
            source,
        });
    }
    if let Err(source) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(ConfigError::Write {
            path: path.to_owned(),
            source,
        });
    }
    Ok(())
}

pub fn check_config(path: Option<PathBuf>) -> Result<(), ConfigError> {
    let (path, explicit) = resolve_config_path(path)?;
    match fs::read_to_string(&path) {
        Ok(contents) => {
            parse_config(&contents, &path)?;
            println!("Configuration is valid: {}", path.display());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound && !explicit => {
            println!("No configuration file found; built-in defaults are active.");
        }
        Err(source) => return Err(ConfigError::Read { path, source }),
    }
    Ok(())
}

fn resolve_config_path(explicit_path: Option<PathBuf>) -> Result<(PathBuf, bool), ConfigError> {
    if let Some(path) = explicit_path {
        return Ok((path, true));
    }
    if let Some(path) = env::var_os(CONFIG_ENV) {
        return Ok((PathBuf::from(path), true));
    }
    let directories = ProjectDirs::from("org", "tuilane", "tuilane")
        .ok_or(ConfigError::ConfigDirectoryUnavailable)?;
    Ok((directories.config_dir().join("tuilane.toml"), false))
}

fn parse_config(contents: &str, path: &Path) -> Result<WorkspaceConfig, ConfigError> {
    let application = parse_application_config(contents, path)?;
    Ok(application.workspace)
}

fn parse_application_config(contents: &str, path: &Path) -> Result<ApplicationConfig, ConfigError> {
    let parsed = toml::from_str::<FileConfig>(contents).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })?;
    parsed.application(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuilane_plugin_api::{DashboardSize, DashboardWidgetId, DashboardWidgetPreference};

    #[test]
    fn missing_configuration_uses_builtin_presets() {
        let config = parse_config("", Path::new("tuilane.toml")).unwrap();
        assert_eq!(config, WorkspaceConfig::default());
    }

    #[test]
    fn supported_presets_are_parsed() {
        let config = parse_config(
            "version = 1\n\n[ui]\ntheme = 'high-contrast'\nlayout = 'focus'\nkeymap = 'vim'\n",
            Path::new("tuilane.toml"),
        )
        .unwrap();
        assert_eq!(
            config,
            WorkspaceConfig::new(
                ThemePreset::HighContrast,
                LayoutPreset::Focus,
                KeymapPreset::Vim
            )
        );
    }

    #[test]
    fn sidebar_navigation_and_icon_only_mode_are_parsed() {
        let config = parse_config(
            "[ui]\nnavigation = 'sidebar'\nnavigation_collapsed = true\n",
            Path::new("tuilane.toml"),
        )
        .unwrap();

        assert_eq!(config.navigation(), NavigationStyle::Sidebar);
        assert!(config.navigation_collapsed());
    }

    #[test]
    fn assistant_is_disabled_by_default_and_enabled_only_with_a_model() {
        let defaults = parse_application_config("", Path::new("tuilane.toml")).unwrap();
        assert!(defaults.assistant.is_none());

        let enabled = parse_application_config(
            "[assistant]\nenabled = true\nmodel = 'qwen3:8b'\n",
            Path::new("tuilane.toml"),
        )
        .unwrap();
        assert_eq!(
            enabled.assistant.as_ref().map(AssistantConfig::model),
            Some("qwen3:8b")
        );
    }

    #[test]
    fn enabled_assistant_requires_a_model_and_loopback_endpoint() {
        assert!(matches!(
            parse_application_config("[assistant]\nenabled = true\n", Path::new("tuilane.toml")),
            Err(ConfigError::MissingValue {
                key: "assistant.model",
                ..
            })
        ));
        assert!(matches!(
            parse_application_config(
                "[assistant]\nenabled = true\nmodel = 'qwen3'\nendpoint = 'https://example.com'\n",
                Path::new("tuilane.toml")
            ),
            Err(ConfigError::UnsupportedValue {
                key: "assistant.endpoint",
                ..
            })
        ));
    }

    #[test]
    fn invalid_versions_values_and_unknown_fields_are_reported() {
        assert!(matches!(
            parse_config("version = 2", Path::new("tuilane.toml")),
            Err(ConfigError::UnsupportedVersion { version: 2, .. })
        ));
        assert!(matches!(
            parse_config("[ui]\ntheme = 'neon'", Path::new("tuilane.toml")),
            Err(ConfigError::UnsupportedValue {
                key: "ui.theme",
                ..
            })
        ));
        assert!(matches!(
            parse_config("[ui]\nnavigation = 'left'", Path::new("tuilane.toml")),
            Err(ConfigError::UnsupportedValue {
                key: "ui.navigation",
                ..
            })
        ));
        assert!(matches!(
            parse_config("[unknown]\nvalue = true", Path::new("tuilane.toml")),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn workspace_preferences_save_without_losing_other_configuration() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!(
            "tuilane-dashboard-config-{}-{unique}",
            process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("tuilane.toml");
        fs::write(
            &path,
            "# Preserve user notes while the app updates its layout.\nversion = 1\n\n[ui]\ntheme = 'high-contrast' # keep the contrast\nlayout = 'focus'\nkeymap = 'vim'\n\n[assistant]\nenabled = true\nmodel = 'qwen3:8b'\nendpoint = 'http://127.0.0.1:11434'\n",
        )
        .unwrap();

        let source = fs::read_to_string(&path).unwrap();
        let original = parse_application_config(&source, &path).unwrap().workspace;
        let id = DashboardWidgetId::new("tuilane.tasks.open").unwrap();
        let preference = DashboardWidgetPreference::new(id, DashboardSize::Wide);
        let dashboard = DashboardLayout::new(vec![preference]);
        let updated = original
            .clone()
            .with_navigation(NavigationStyle::Sidebar, true)
            .with_dashboard_layout(dashboard.clone());
        save_workspace_preferences(&path, &original, &updated).unwrap();

        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# Preserve user notes"));
        assert!(saved.contains("# keep the contrast"));
        let parsed = parse_application_config(&saved, &path).unwrap();
        assert_eq!(parsed.workspace.dashboard_layout(), &dashboard);
        assert_eq!(parsed.workspace.navigation(), NavigationStyle::Sidebar);
        assert!(parsed.workspace.navigation_collapsed());
        assert_eq!(parsed.workspace.theme(), ThemePreset::HighContrast);
        assert_eq!(
            parsed.assistant.as_ref().map(AssistantConfig::model),
            Some("qwen3:8b")
        );

        save_workspace_preferences(&path, &updated, &updated).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), saved);
        fs::remove_dir_all(directory).unwrap();
    }
}
