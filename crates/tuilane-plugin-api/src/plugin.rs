//! Versioned contracts used to describe and validate external Tuilane plugins.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

/// Version of the manifest structure understood by this release.
pub const PLUGIN_MANIFEST_VERSION: u32 = 1;

/// Version of the WIT interface used by external plugin components.
pub const PLUGIN_API_VERSION: &str = "0.1.0";

/// Host service that returns non-sensitive application and API version metadata.
pub const WORKSPACE_INFO_CAPABILITY: &str = "workspace.info@1";

/// Host service that performs permission-checked workspace search.
pub const WORKSPACE_SEARCH_CAPABILITY: &str = "workspace.search@1";

/// Permission required for `workspace.info@1`.
pub const WORKSPACE_INFO_PERMISSION: &str = "workspace.read";

/// Permission required for `workspace.search@1`.
pub const WORKSPACE_SEARCH_PERMISSION: &str = "workspace.search";

/// Validated, globally namespaced identity for an external plugin.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PluginId(Box<str>);

impl PluginId {
    /// Creates a namespaced lowercase identifier such as `org.example.hello`.
    pub fn new(value: impl AsRef<str>) -> Result<Self, PluginIdError> {
        let value = value.as_ref();
        let valid = value.len() <= 128
            && value.split('.').count() >= 2
            && value.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.len() <= 63
                    && segment.as_bytes()[0].is_ascii_lowercase()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte)
                    })
            });
        if !valid {
            return Err(PluginIdError);
        }
        Ok(Self(value.into()))
    }

    /// Returns the validated identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Error returned when a plugin identifier is malformed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PluginIdError;

impl fmt::Display for PluginIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "plugin IDs must have 2 or more lowercase dot-separated segments using letters, digits, '-' or '_'",
        )
    }
}

impl Error for PluginIdError {}

/// Manifest for one installable WebAssembly component plugin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PluginManifest {
    /// Manifest structure version.
    pub manifest_version: u32,
    /// Globally namespaced plugin identity.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Plugin release version in SemVer format.
    pub version: String,
    /// Exact WIT API version this component implements.
    pub api_version: String,
    /// Component filename; this release requires `plugin.wasm`.
    pub component: String,
    /// Host capabilities the plugin may request and their required grants.
    #[serde(default)]
    pub services: Vec<PluginServiceRequest>,
    /// Commands exported by the plugin.
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
    /// Views contributed by the plugin.
    #[serde(default)]
    pub views: Vec<PluginView>,
}

/// A requested host capability and its exact user-controlled permission grant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginServiceRequest {
    /// Versioned host capability identifier, for example `workspace.info@1`.
    pub id: String,
    /// Permission identifier required for each call to the capability.
    pub permission: String,
}

/// A command exported by an external plugin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCommand {
    /// Namespaced operation identifier, for example `org.example.hello.greet@1`.
    pub id: String,
    /// Short user-facing description.
    pub summary: String,
    /// Declared side-effect class.
    pub effect: PluginEffect,
}

/// Side-effect class declared for an external command.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginEffect {
    /// Does not change workspace or plugin state.
    ReadOnly,
    /// Changes state but is reversible or non-destructive.
    Mutating,
    /// Removes or irreversibly changes data.
    Destructive,
}

/// A view exported by an external plugin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginView {
    /// Namespaced view identifier.
    pub id: String,
    /// Title shown in navigation.
    pub title: String,
    /// Short description shown in the workspace shell.
    pub description: String,
}

/// One action returned by a plugin's view model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginAction {
    id: Box<str>,
    title: Box<str>,
    description: Box<str>,
}

impl PluginAction {
    /// Creates an action returned from an external plugin.
    pub fn new(
        id: impl Into<Box<str>>,
        title: impl Into<Box<str>>,
        description: impl Into<Box<str>>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
        }
    }

    /// Returns the action's stable plugin-local identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the user-facing action title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the action description.
    pub fn description(&self) -> &str {
        &self.description
    }
}

/// Host-rendered, bounded text model returned by an external plugin view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginViewModel {
    title: Box<str>,
    lines: Vec<Box<str>>,
    actions: Vec<PluginAction>,
}

impl PluginViewModel {
    /// Creates a view model; the runtime validates size and control-character limits.
    pub fn new(
        title: impl Into<Box<str>>,
        lines: Vec<Box<str>>,
        actions: Vec<PluginAction>,
    ) -> Self {
        Self {
            title: title.into(),
            lines,
            actions,
        }
    }

    /// Returns the plugin-provided title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the plugin-provided display lines.
    pub fn lines(&self) -> &[Box<str>] {
        &self.lines
    }

    /// Returns the actions available from this view.
    pub fn actions(&self) -> &[PluginAction] {
        &self.actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_ids_are_namespaced_and_safe_for_storage_paths() {
        assert_eq!(
            PluginId::new("org.example.hello").unwrap().as_str(),
            "org.example.hello"
        );
        for value in [
            "",
            "hello",
            "org..hello",
            "org.example/hello",
            "../hello",
            "Org.example",
        ] {
            assert!(PluginId::new(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn manifests_use_kebab_case_and_reject_unknown_fields() {
        let manifest = r#"
manifest-version = 1
id = "org.example.hello"
name = "Hello"
version = "0.1.0"
api-version = "0.1.0"
component = "plugin.wasm"

[[services]]
id = "workspace.info@1"
permission = "workspace.read"

[[commands]]
id = "org.example.hello.greet@1"
summary = "Greet the user."
effect = "read-only"

[[views]]
id = "org.example.hello.main"
title = "Hello"
description = "Example plugin view."
"#;
        let parsed: PluginManifest = toml::from_str(manifest).unwrap();
        assert_eq!(parsed.api_version, PLUGIN_API_VERSION);
        assert_eq!(parsed.commands[0].effect, PluginEffect::ReadOnly);
        assert!(
            toml::from_str::<PluginManifest>(&format!("{manifest}\nunknown = true\n")).is_err()
        );
    }
}
