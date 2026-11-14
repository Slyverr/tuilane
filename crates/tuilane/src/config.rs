use std::{
    env,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::Deserialize;
use tuilane_assistant::{AssistantConfig, AssistantConfigError, DEFAULT_ENDPOINT};
use tuilane_ui::{KeymapPreset, LayoutPreset, ThemePreset, WorkspaceConfig};

const CONFIG_ENV: &str = "TUILANE_CONFIG";
const CONFIG_VERSION: u32 = 1;

#[derive(Debug)]
pub enum ConfigError {
    ConfigDirectoryUnavailable,
    Read {
        path: PathBuf,
        source: io::Error,
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
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UiConfig {
    theme: String,
    layout: String,
    keymap: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: ThemePreset::default().as_str().to_owned(),
            layout: LayoutPreset::default().as_str().to_owned(),
            keymap: KeymapPreset::default().as_str().to_owned(),
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
        Ok(WorkspaceConfig::new(theme, layout, keymap))
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
            workspace,
            assistant,
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
            Ok(ApplicationConfig {
                workspace: WorkspaceConfig::default(),
                assistant: None,
            })
        }
        Err(source) => Err(ConfigError::Read { path, source }),
    }
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
            parse_config("[unknown]\nvalue = true", Path::new("tuilane.toml")),
            Err(ConfigError::Parse { .. })
        ));
    }
}
