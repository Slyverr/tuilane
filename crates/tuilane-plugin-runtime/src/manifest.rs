use semver::Version;
use std::path::PathBuf;
use thiserror::Error;
use tuilane_plugin_api::plugin::{
    PLUGIN_API_VERSION, PLUGIN_MANIFEST_VERSION, PluginId, PluginManifest,
};

pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const COMPONENT_FILENAME: &str = "plugin.wasm";
const MAX_PLUGIN_NAME_CHARS: usize = 80;
const MAX_DESCRIPTION_CHARS: usize = 256;
const MAX_COMMANDS: usize = 128;
const MAX_SERVICES: usize = 64;
const MAX_VIEWS: usize = 64;

/// Manifest validation failure with a stable compatibility classification.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// The TOML document could not be parsed or contained unknown fields.
    #[error("invalid plugin manifest: {0}")]
    Parse(#[from] toml::de::Error),
    /// The plugin manifest schema is not supported by this host.
    #[error("manifest version {0} is unsupported; expected {PLUGIN_MANIFEST_VERSION}")]
    UnsupportedManifestVersion(u32),
    /// The plugin requests an unsupported WIT API version.
    #[error("plugin API version {found} is incompatible; this host supports {PLUGIN_API_VERSION}")]
    IncompatibleApiVersion { found: String },
    /// The plugin ID is invalid or unsafe to use in a managed storage path.
    #[error("{0}")]
    InvalidPluginId(#[from] tuilane_plugin_api::plugin::PluginIdError),
    /// The plugin release version is not valid SemVer.
    #[error("plugin version is not valid SemVer: {0}")]
    InvalidPluginVersion(#[from] semver::Error),
    /// The manifest points outside its own bundle or uses an unsupported filename.
    #[error("component must be named '{COMPONENT_FILENAME}'")]
    InvalidComponentPath,
    /// A display name or description failed validation.
    #[error(
        "invalid {field}: it must be non-empty, at most {max} characters, and contain no control characters"
    )]
    InvalidText { field: &'static str, max: usize },
    /// A declared ID or permission is malformed or outside the plugin namespace.
    #[error("invalid {field} identifier '{value}'")]
    InvalidIdentifier { field: &'static str, value: String },
    /// A manifest contains duplicate IDs.
    #[error("duplicate {field} identifier '{value}'")]
    DuplicateIdentifier { field: &'static str, value: String },
    /// One manifest section exceeds its bound.
    #[error("plugin manifest contains too many {field}; maximum is {max}")]
    TooMany { field: &'static str, max: usize },
    /// The manifest file could not be read safely.
    #[error("could not read manifest {path}: {reason}")]
    Read { path: PathBuf, reason: String },
    /// The manifest path is not a regular file.
    #[error("manifest {0} is not a regular file")]
    NotRegular(PathBuf),
    /// The manifest file exceeds its size limit.
    #[error("manifest {path} exceeds {max} bytes")]
    FileTooLarge { path: PathBuf, max: usize },
}

/// Parses and validates the portable manifest shared by the host and SDK.
pub fn validate_manifest(source: &str) -> Result<PluginManifest, ManifestError> {
    if source.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestError::InvalidText {
            field: "manifest",
            max: MAX_MANIFEST_BYTES,
        });
    }

    let manifest: PluginManifest = toml::from_str(source)?;
    if manifest.manifest_version != PLUGIN_MANIFEST_VERSION {
        return Err(ManifestError::UnsupportedManifestVersion(
            manifest.manifest_version,
        ));
    }

    let id = PluginId::new(&manifest.id)?;
    if manifest.api_version != PLUGIN_API_VERSION {
        return Err(ManifestError::IncompatibleApiVersion {
            found: manifest.api_version,
        });
    }
    Version::parse(&manifest.version)?;
    if manifest.component != COMPONENT_FILENAME {
        return Err(ManifestError::InvalidComponentPath);
    }
    validate_text("name", &manifest.name, MAX_PLUGIN_NAME_CHARS)?;
    validate_count("services", manifest.services.len(), MAX_SERVICES)?;
    validate_count("commands", manifest.commands.len(), MAX_COMMANDS)?;
    validate_count("views", manifest.views.len(), MAX_VIEWS)?;

    let mut ids = std::collections::HashSet::new();
    for service in &manifest.services {
        validate_capability_id(&service.id)?;
        validate_permission(&service.permission)?;
        if !ids.insert(service.id.as_str()) {
            return Err(ManifestError::DuplicateIdentifier {
                field: "service",
                value: service.id.clone(),
            });
        }
    }

    ids.clear();
    for command in &manifest.commands {
        validate_plugin_contract_id("command", &command.id, id.as_str())?;
        validate_text("command summary", &command.summary, MAX_DESCRIPTION_CHARS)?;
        if !ids.insert(command.id.as_str()) {
            return Err(ManifestError::DuplicateIdentifier {
                field: "command",
                value: command.id.clone(),
            });
        }
    }

    ids.clear();
    for view in &manifest.views {
        validate_namespaced_id("view", &view.id, id.as_str())?;
        validate_text("view title", &view.title, MAX_PLUGIN_NAME_CHARS)?;
        validate_text("view description", &view.description, MAX_DESCRIPTION_CHARS)?;
        if !ids.insert(view.id.as_str()) {
            return Err(ManifestError::DuplicateIdentifier {
                field: "view",
                value: view.id.clone(),
            });
        }
    }

    Ok(manifest)
}

fn validate_count(field: &'static str, count: usize, max: usize) -> Result<(), ManifestError> {
    if count > max {
        return Err(ManifestError::TooMany { field, max });
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), ManifestError> {
    if value.trim().is_empty() || value.chars().count() > max || value.chars().any(char::is_control)
    {
        return Err(ManifestError::InvalidText { field, max });
    }
    Ok(())
}

fn validate_capability_id(value: &str) -> Result<(), ManifestError> {
    let Some((name, version)) = value.rsplit_once('@') else {
        return Err(invalid_id("service", value));
    };
    if !valid_namespaced_segments(name) || !valid_contract_version(version) {
        return Err(invalid_id("service", value));
    }
    Ok(())
}

fn validate_permission(value: &str) -> Result<(), ManifestError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return Err(invalid_id("permission", value));
    }
    Ok(())
}

fn validate_plugin_contract_id(
    field: &'static str,
    value: &str,
    plugin_id: &str,
) -> Result<(), ManifestError> {
    let Some((name, version)) = value.rsplit_once('@') else {
        return Err(invalid_id(field, value));
    };
    if !name.starts_with(&format!("{plugin_id}."))
        || !valid_namespaced_segments(name)
        || !valid_contract_version(version)
    {
        return Err(invalid_id(field, value));
    }
    Ok(())
}

fn validate_namespaced_id(
    field: &'static str,
    value: &str,
    plugin_id: &str,
) -> Result<(), ManifestError> {
    if !value.starts_with(&format!("{plugin_id}.")) || !valid_namespaced_segments(value) {
        return Err(invalid_id(field, value));
    }
    Ok(())
}

fn valid_namespaced_segments(value: &str) -> bool {
    value.len() <= 192
        && value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.len() <= 63
                && segment.as_bytes()[0].is_ascii_lowercase()
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte)
                })
        })
}

fn valid_contract_version(value: &str) -> bool {
    matches!(value.parse::<u16>(), Ok(version) if version > 0)
}

fn invalid_id(field: &'static str, value: &str) -> ManifestError {
    ManifestError::InvalidIdentifier {
        field,
        value: value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
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

    #[test]
    fn validates_compatible_manifest_and_namespaced_contributions() {
        let manifest = validate_manifest(VALID).expect("manifest should be valid");
        assert_eq!(manifest.id, "org.example.hello");
        assert_eq!(manifest.services.len(), 1);
        assert_eq!(manifest.commands.len(), 1);
        assert_eq!(manifest.views.len(), 1);
    }

    #[test]
    fn rejects_incompatible_versions_and_unsafe_component_paths() {
        assert!(matches!(
            validate_manifest(&VALID.replace("manifest-version = 1", "manifest-version = 2")),
            Err(ManifestError::UnsupportedManifestVersion(2))
        ));
        assert!(matches!(
            validate_manifest(&VALID.replace("api-version = \"0.1.0\"", "api-version = \"1.0.0\"")),
            Err(ManifestError::IncompatibleApiVersion { .. })
        ));
        assert!(matches!(
            validate_manifest(&VALID.replace(
                "component = \"plugin.wasm\"",
                "component = \"../plugin.wasm\""
            )),
            Err(ManifestError::InvalidComponentPath)
        ));
    }

    #[test]
    fn rejects_invalid_permission_and_contributions_outside_plugin_namespace() {
        assert!(validate_manifest(&VALID.replace("workspace.read", "../workspace.read")).is_err());
        assert!(
            validate_manifest(
                &VALID.replace("org.example.hello.greet@1", "org.other.plugin.greet@1")
            )
            .is_err()
        );
        assert!(
            validate_manifest(&VALID.replace("org.example.hello.main", "org.other.plugin.main"))
                .is_err()
        );
    }
}
