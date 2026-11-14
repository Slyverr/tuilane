use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

use thiserror::Error;
use tuilane_plugin_api::plugin::{
    PluginCommand, PluginId, PluginManifest, PluginServiceRequest, PluginView,
};
use uuid::Uuid;
use wasmtime::Engine;

use crate::{
    component::{ComponentError, HostServiceDispatcher, PluginComponent, create_engine},
    manifest::{COMPONENT_FILENAME, MAX_MANIFEST_BYTES, ManifestError, validate_manifest},
    state::{self, PluginState},
};

const MANIFEST_FILENAME: &str = "manifest.toml";

/// User-managed directories for installed plugin bundles and state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginPaths {
    /// Installed manifests and Wasm components.
    pub install_dir: PathBuf,
    /// Enablement and permission grant files.
    pub state_dir: PathBuf,
    /// Host-owned plugin data, intentionally preserved by uninstall.
    pub data_dir: PathBuf,
}

impl PluginPaths {
    /// Creates a plugin store from application-selected paths.
    pub fn new(
        install_dir: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
        data_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            install_dir: install_dir.into(),
            state_dir: state_dir.into(),
            data_dir: data_dir.into(),
        }
    }
}

/// Error returned by plugin management or Wasm execution.
#[derive(Debug, Error)]
pub enum PluginRuntimeError {
    /// Filesystem operation failed.
    #[error("could not {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    /// Plugin manifest parsing or validation failed.
    #[error("plugin manifest {path}: {source}")]
    Manifest {
        path: PathBuf,
        source: ManifestError,
    },
    /// Persisted plugin state could not be loaded or updated.
    #[error("plugin state is invalid: {0}")]
    State(String),
    /// The package is already installed.
    #[error("plugin '{0}' is already installed")]
    AlreadyInstalled(String),
    /// The requested plugin is not installed.
    #[error("plugin '{0}' is not installed")]
    NotInstalled(String),
    /// The plugin is not enabled.
    #[error("plugin '{0}' is disabled")]
    Disabled(String),
    /// The manifest identity does not match its installed directory.
    #[error("plugin manifest identity does not match installed directory '{0}'")]
    IdentityMismatch(String),
    /// The permission is not requested by the plugin manifest.
    #[error("plugin '{plugin}' does not request permission '{permission}'")]
    PermissionNotRequested { plugin: String, permission: String },
    /// The component requires a host capability unavailable in this release.
    #[error("plugin '{plugin}' requires unsupported host capability '{capability}'")]
    UnsupportedCapability { plugin: String, capability: String },
    /// Component compilation, validation, instantiation, or invocation failed.
    #[error("plugin component failed: {0}")]
    Component(String),
    /// A component was unexpectedly removed between lifecycle validation and dispatch.
    #[error("plugin '{0}' is not loaded")]
    ComponentNotLoaded(String),
    /// An enabled plugin did not contribute the requested operation.
    #[error("plugin '{plugin}' does not provide {kind} '{id}'")]
    ContributionNotFound {
        plugin: String,
        kind: &'static str,
        id: String,
    },
    /// An action returned by the view model is no longer available.
    #[error("plugin view action '{action}' is unavailable")]
    ActionUnavailable { action: String },
}

impl From<ComponentError> for PluginRuntimeError {
    fn from(error: ComponentError) -> Self {
        Self::Component(error.to_string())
    }
}

/// Summary of an installed plugin for management and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginStatus {
    /// Installed folder name or validated plugin ID.
    pub id: String,
    /// Manifest display name, or the folder name when the manifest is invalid.
    pub name: String,
    /// Plugin release version, or an empty string when unavailable.
    pub version: String,
    /// Whether the plugin is enabled in persisted state.
    pub enabled: bool,
    /// Persisted permission grants in stable order.
    pub grants: Vec<String>,
    /// Manifest, state, or compatibility error if the plugin cannot be loaded.
    pub error: Option<String>,
}

/// A host-validated UI view contribution from one loaded plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeView {
    /// Owning plugin identity.
    pub plugin_id: PluginId,
    /// View metadata validated against the component exports.
    pub view: PluginView,
}

/// A recoverable load failure for one enabled plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginLoadFailure {
    /// Plugin folder name.
    pub plugin_id: String,
    /// User-readable reason this plugin did not load.
    pub error: String,
}

/// Installs, validates, enables, and dispatches external plugins.
pub struct PluginManager {
    paths: PluginPaths,
    host_services: Rc<RefCell<dyn HostServiceDispatcher>>,
    engine: Option<Engine>,
    loaded: HashMap<PluginId, PluginComponent>,
}

impl PluginManager {
    /// Creates a manager without touching the filesystem or starting Wasmtime.
    pub fn new(paths: PluginPaths, host_services: Rc<RefCell<dyn HostServiceDispatcher>>) -> Self {
        Self {
            paths,
            host_services,
            engine: None,
            loaded: HashMap::new(),
        }
    }

    /// Validates a bundle and copies only its manifest and component into managed storage.
    pub fn install(&mut self, bundle: impl AsRef<Path>) -> Result<PluginId, PluginRuntimeError> {
        let bundle = bundle.as_ref();
        let metadata =
            fs::symlink_metadata(bundle).map_err(|source| io_error("inspect", bundle, source))?;
        if !metadata.file_type().is_dir() {
            return Err(PluginRuntimeError::Io {
                action: "inspect plugin bundle directory",
                path: bundle.to_owned(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "not a regular directory"),
            });
        }

        let manifest_path = bundle.join(MANIFEST_FILENAME);
        let manifest_source = read_limited_regular_file(&manifest_path, MAX_MANIFEST_BYTES as u64)?;
        let manifest_text =
            std::str::from_utf8(&manifest_source).map_err(|source| PluginRuntimeError::Io {
                action: "decode plugin manifest",
                path: manifest_path.clone(),
                source: io::Error::new(io::ErrorKind::InvalidData, source),
            })?;
        let manifest =
            validate_manifest(manifest_text).map_err(|source| PluginRuntimeError::Manifest {
                path: manifest_path.clone(),
                source,
            })?;
        let plugin_id =
            PluginId::new(&manifest.id).map_err(|source| PluginRuntimeError::Manifest {
                path: manifest_path.clone(),
                source: ManifestError::InvalidPluginId(source),
            })?;
        self.validate_capabilities(&manifest)?;

        let component_path = bundle.join(COMPONENT_FILENAME);
        let destination = self.install_path(&plugin_id);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err(PluginRuntimeError::AlreadyInstalled(plugin_id.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(io_error(
                    "inspect install destination",
                    &destination,
                    source,
                ));
            }
        }

        fs::create_dir_all(&self.paths.install_dir).map_err(|source| {
            io_error(
                "create plugin install directory",
                &self.paths.install_dir,
                source,
            )
        })?;
        fs::create_dir_all(&self.paths.state_dir).map_err(|source| {
            io_error(
                "create plugin state directory",
                &self.paths.state_dir,
                source,
            )
        })?;
        let state_path = self.state_path(&plugin_id);
        match fs::symlink_metadata(&state_path) {
            Ok(_) => return Err(PluginRuntimeError::AlreadyInstalled(plugin_id.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(io_error("inspect plugin state", &state_path, source)),
        }
        let staging = self
            .paths
            .install_dir
            .join(format!(".install-{}", Uuid::new_v4()));
        fs::create_dir(&staging)
            .map_err(|source| io_error("create plugin staging directory", &staging, source))?;

        let mut state_written = false;
        let result = (|| {
            write_new_file(&staging.join(MANIFEST_FILENAME), &manifest_source)?;
            let component_source =
                read_limited_regular_file(&component_path, crate::component::MAX_COMPONENT_BYTES)?;
            write_new_file(&staging.join(COMPONENT_FILENAME), &component_source)?;
            let engine = self.engine()?;
            PluginComponent::load(
                &engine,
                &staging.join(COMPONENT_FILENAME),
                state_path.clone(),
                manifest.clone(),
                self.host_services.clone(),
            )?;
            state::write(&state_path, &PluginState::default())
                .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
            state_written = true;
            fs::rename(&staging, &destination)
                .map_err(|source| io_error("install plugin bundle", &destination, source))?;
            Ok::<_, PluginRuntimeError>(())
        })();

        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
            if state_written {
                let _ = fs::remove_file(&state_path);
            }
        }
        result?;
        Ok(plugin_id)
    }

    /// Lists installed bundles, including compatibility and state errors.
    pub fn list(&self) -> Result<Vec<PluginStatus>, PluginRuntimeError> {
        let entries = match fs::read_dir(&self.paths.install_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(io_error(
                    "read plugin install directory",
                    &self.paths.install_dir,
                    source,
                ));
            }
        };

        let mut statuses = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| {
                io_error(
                    "read plugin directory entry",
                    &self.paths.install_dir,
                    source,
                )
            })?;
            let file_type = entry.file_type().map_err(|source| {
                io_error("inspect plugin directory entry", &entry.path(), source)
            })?;
            if !file_type.is_dir() {
                continue;
            }

            let folder = entry.file_name().to_string_lossy().into_owned();
            if folder.starts_with('.') {
                continue;
            }
            statuses.push(self.status_for_folder(&folder));
        }
        statuses.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(statuses)
    }

    /// Enables a compatible plugin after instantiating and validating its exports.
    pub fn enable(&mut self, value: &str) -> Result<(), PluginRuntimeError> {
        let plugin_id = PluginId::new(value).map_err(|error| PluginRuntimeError::Manifest {
            path: self.paths.install_dir.join(value).join(MANIFEST_FILENAME),
            source: ManifestError::InvalidPluginId(error),
        })?;
        self.ensure_installed(&plugin_id)?;
        let state_path = self.state_path(&plugin_id);
        let mut plugin_state = state::read(&state_path)
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        if plugin_state.is_enabled() && self.loaded.contains_key(&plugin_id) {
            return Ok(());
        }

        let component = self.load_component(&plugin_id)?;
        plugin_state.set_enabled(true);
        state::write(&state_path, &plugin_state)
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        self.loaded.insert(plugin_id, component);
        Ok(())
    }

    /// Disables a plugin and unloads its component while preserving grants and data.
    pub fn disable(&mut self, value: &str) -> Result<(), PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        self.ensure_installed(&plugin_id)?;
        let path = self.state_path(&plugin_id);
        let mut plugin_state =
            state::read(&path).map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        plugin_state.set_enabled(false);
        state::write(&path, &plugin_state)
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        self.loaded.remove(&plugin_id);
        Ok(())
    }

    /// Grants a permission requested by the installed manifest.
    pub fn grant(&mut self, value: &str, permission: &str) -> Result<(), PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        let manifest = self.read_installed_manifest(&plugin_id)?;
        if !manifest
            .services
            .iter()
            .any(|service| service.permission == permission)
        {
            return Err(PluginRuntimeError::PermissionNotRequested {
                plugin: plugin_id.to_string(),
                permission: permission.to_owned(),
            });
        }
        let path = self.state_path(&plugin_id);
        let mut plugin_state =
            state::read(&path).map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        plugin_state.grant(permission);
        state::write(&path, &plugin_state)
            .map_err(|error| PluginRuntimeError::State(error.to_string()))
    }

    /// Revokes a permission requested by the installed manifest.
    pub fn revoke(&mut self, value: &str, permission: &str) -> Result<bool, PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        let manifest = self.read_installed_manifest(&plugin_id)?;
        if !manifest
            .services
            .iter()
            .any(|service| service.permission == permission)
        {
            return Err(PluginRuntimeError::PermissionNotRequested {
                plugin: plugin_id.to_string(),
                permission: permission.to_owned(),
            });
        }
        let path = self.state_path(&plugin_id);
        let mut plugin_state =
            state::read(&path).map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        let removed = plugin_state.revoke(permission);
        state::write(&path, &plugin_state)
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        Ok(removed)
    }

    /// Removes only the installed bundle and its grants; host-owned plugin data remains.
    pub fn uninstall(&mut self, value: &str) -> Result<(), PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        self.ensure_installed(&plugin_id)?;
        let bundle = self.install_path(&plugin_id);
        let state_path = self.state_path(&plugin_id);
        let mut plugin_state = state::read(&state_path)
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        plugin_state.set_enabled(false);
        state::write(&state_path, &plugin_state)
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        self.loaded.remove(&plugin_id);
        fs::remove_dir_all(&bundle)
            .map_err(|source| io_error("remove plugin bundle", &bundle, source))?;
        match fs::remove_file(&state_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(io_error("remove plugin state", &state_path, source)),
        }
        Ok(())
    }

    /// Loads every enabled compatible plugin and returns failures without stopping the host.
    pub fn load_enabled(&mut self) -> Result<Vec<PluginLoadFailure>, PluginRuntimeError> {
        let statuses = self.list()?;
        let mut failures = Vec::new();
        for status in statuses.into_iter().filter(|status| status.enabled) {
            if let Some(error) = status.error {
                failures.push(PluginLoadFailure {
                    plugin_id: status.id,
                    error,
                });
                continue;
            }
            let plugin_id = match self.plugin_id(&status.id) {
                Ok(plugin_id) => plugin_id,
                Err(error) => {
                    failures.push(PluginLoadFailure {
                        plugin_id: status.id,
                        error: error.to_string(),
                    });
                    continue;
                }
            };
            if let Err(error) = self.ensure_loaded(&plugin_id) {
                failures.push(PluginLoadFailure {
                    plugin_id: status.id,
                    error: error.to_string(),
                });
            }
        }
        Ok(failures)
    }

    /// Returns views from successfully loaded plugins in stable plugin/view order.
    pub fn views(&self) -> Vec<RuntimeView> {
        let mut views = self
            .loaded
            .iter()
            .flat_map(|(plugin_id, component)| {
                component
                    .manifest()
                    .views
                    .iter()
                    .cloned()
                    .map(|view| RuntimeView {
                        plugin_id: plugin_id.clone(),
                        view,
                    })
            })
            .collect::<Vec<_>>();
        views.sort_by(|left, right| left.view.id.cmp(&right.view.id));
        views
    }

    /// Returns the commands declared by one installed plugin, including while disabled.
    pub fn commands(&self, value: &str) -> Result<Vec<PluginCommand>, PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        let mut commands = self.read_installed_manifest(&plugin_id)?.commands;
        commands.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(commands)
    }

    /// Returns the host services and permission names requested by one plugin.
    pub fn services(&self, value: &str) -> Result<Vec<PluginServiceRequest>, PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        let mut services = self.read_installed_manifest(&plugin_id)?.services;
        services.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(services)
    }

    /// Returns whether a plugin remains enabled in the shared state file.
    pub fn is_enabled(&self, value: &str) -> Result<bool, PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        self.ensure_installed(&plugin_id)?;
        let plugin_state = state::read(&self.state_path(&plugin_id))
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        Ok(plugin_state.is_enabled())
    }

    /// Returns whether each listed permission is currently granted.
    pub fn has_permissions(
        &self,
        value: &str,
        permissions: &[String],
    ) -> Result<bool, PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        self.ensure_installed(&plugin_id)?;
        let plugin_state = state::read(&self.state_path(&plugin_id))
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        Ok(permissions
            .iter()
            .all(|permission| plugin_state.has_permission(permission)))
    }

    /// Invokes a manifest-declared plugin command with a JSON request.
    pub fn invoke(
        &mut self,
        value: &str,
        command_id: &str,
        request: &str,
    ) -> Result<String, PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        self.ensure_enabled(&plugin_id)?;
        self.ensure_loaded(&plugin_id)?;
        let result = {
            let component = self
                .loaded
                .get_mut(&plugin_id)
                .ok_or_else(|| PluginRuntimeError::ComponentNotLoaded(plugin_id.to_string()))?;
            if !component
                .manifest()
                .commands
                .iter()
                .any(|command| command.id == command_id)
            {
                return Err(PluginRuntimeError::ContributionNotFound {
                    plugin: plugin_id.to_string(),
                    kind: "command",
                    id: command_id.to_owned(),
                });
            }
            component.invoke(command_id, request)
        };
        if result.is_err() {
            self.loaded.remove(&plugin_id);
        }
        result.map_err(Into::into)
    }

    /// Renders a manifest-declared view through the guest component.
    pub fn render_view(
        &mut self,
        value: &str,
        view_id: &str,
    ) -> Result<tuilane_plugin_api::plugin::PluginViewModel, PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        self.ensure_enabled(&plugin_id)?;
        self.ensure_loaded(&plugin_id)?;
        let result = {
            let component = self
                .loaded
                .get_mut(&plugin_id)
                .ok_or_else(|| PluginRuntimeError::ComponentNotLoaded(plugin_id.to_string()))?;
            if !component
                .manifest()
                .views
                .iter()
                .any(|view| view.id == view_id)
            {
                return Err(PluginRuntimeError::ContributionNotFound {
                    plugin: plugin_id.to_string(),
                    kind: "view",
                    id: view_id.to_owned(),
                });
            }
            component.render(view_id)
        };
        if result.is_err() {
            self.loaded.remove(&plugin_id);
        }
        result.map_err(Into::into)
    }

    /// Activates an action that appeared in the current view model.
    pub fn activate_view(
        &mut self,
        value: &str,
        view_id: &str,
        action_id: &str,
        request: &str,
    ) -> Result<tuilane_plugin_api::plugin::PluginViewModel, PluginRuntimeError> {
        let plugin_id = self.plugin_id(value)?;
        self.ensure_enabled(&plugin_id)?;
        self.ensure_loaded(&plugin_id)?;
        let result = (|| {
            let component = self
                .loaded
                .get_mut(&plugin_id)
                .ok_or_else(|| PluginRuntimeError::ComponentNotLoaded(plugin_id.to_string()))?;
            if !component
                .manifest()
                .views
                .iter()
                .any(|view| view.id == view_id)
            {
                return Err(PluginRuntimeError::ContributionNotFound {
                    plugin: plugin_id.to_string(),
                    kind: "view",
                    id: view_id.to_owned(),
                });
            }
            let current = component.render(view_id)?;
            if !current
                .actions()
                .iter()
                .any(|action| action.id() == action_id)
            {
                return Err(PluginRuntimeError::ActionUnavailable {
                    action: action_id.to_owned(),
                });
            }
            component
                .activate(view_id, action_id, request)
                .map_err(Into::into)
        })();
        if result.is_err() {
            self.loaded.remove(&plugin_id);
        }
        result
    }

    fn engine(&mut self) -> Result<Engine, PluginRuntimeError> {
        if let Some(engine) = &self.engine {
            return Ok(engine.clone());
        }
        let engine = create_engine()?;
        self.engine = Some(engine.clone());
        Ok(engine)
    }

    fn ensure_loaded(&mut self, plugin_id: &PluginId) -> Result<(), PluginRuntimeError> {
        self.ensure_enabled(plugin_id)?;
        if self.loaded.contains_key(plugin_id) {
            return Ok(());
        }
        let component = self.load_component(plugin_id)?;
        self.loaded.insert(plugin_id.clone(), component);
        Ok(())
    }

    fn ensure_enabled(&self, plugin_id: &PluginId) -> Result<(), PluginRuntimeError> {
        self.ensure_installed(plugin_id)?;
        let plugin_state = state::read(&self.state_path(plugin_id))
            .map_err(|error| PluginRuntimeError::State(error.to_string()))?;
        if !plugin_state.is_enabled() {
            return Err(PluginRuntimeError::Disabled(plugin_id.to_string()));
        }
        Ok(())
    }

    fn load_component(
        &mut self,
        plugin_id: &PluginId,
    ) -> Result<PluginComponent, PluginRuntimeError> {
        let manifest = self.read_installed_manifest(plugin_id)?;
        self.validate_capabilities(&manifest)?;
        let engine = self.engine()?;
        PluginComponent::load(
            &engine,
            &self.install_path(plugin_id).join(COMPONENT_FILENAME),
            self.state_path(plugin_id),
            manifest,
            self.host_services.clone(),
        )
        .map_err(Into::into)
    }

    fn validate_capabilities(&self, manifest: &PluginManifest) -> Result<(), PluginRuntimeError> {
        let services = self.host_services.borrow();
        for service in &manifest.services {
            if !services.supports(&service.id) {
                return Err(PluginRuntimeError::UnsupportedCapability {
                    plugin: manifest.id.clone(),
                    capability: service.id.clone(),
                });
            }
        }
        Ok(())
    }

    fn read_installed_manifest(
        &self,
        plugin_id: &PluginId,
    ) -> Result<PluginManifest, PluginRuntimeError> {
        let bundle = self.install_path(plugin_id);
        self.ensure_installed(plugin_id)?;
        let path = bundle.join(MANIFEST_FILENAME);
        let manifest = read_manifest(&path).map_err(|source| PluginRuntimeError::Manifest {
            path: path.clone(),
            source,
        })?;
        if manifest.id != plugin_id.as_str() {
            return Err(PluginRuntimeError::IdentityMismatch(plugin_id.to_string()));
        }
        Ok(manifest)
    }

    fn status_for_folder(&self, folder: &str) -> PluginStatus {
        let path = self.paths.install_dir.join(folder).join(MANIFEST_FILENAME);
        let mut status = PluginStatus {
            id: folder.to_owned(),
            name: folder.to_owned(),
            version: String::new(),
            enabled: false,
            grants: Vec::new(),
            error: None,
        };
        let manifest = match read_manifest(&path) {
            Ok(manifest) => manifest,
            Err(error) => {
                status.error = Some(error.to_string());
                return status;
            }
        };
        status.name = manifest.name.clone();
        status.version = manifest.version.clone();
        if manifest.id != folder {
            status.error = Some("plugin ID does not match its install directory".to_owned());
        } else if let Err(error) = self.validate_capabilities(&manifest) {
            status.error = Some(error.to_string());
        }

        let plugin_id = match PluginId::new(folder) {
            Ok(plugin_id) => plugin_id,
            Err(error) => {
                status.error = Some(error.to_string());
                return status;
            }
        };
        match state::read(&self.state_path(&plugin_id)) {
            Ok(plugin_state) => {
                status.enabled = plugin_state.is_enabled();
                status.grants = plugin_state.grants().map(str::to_owned).collect();
            }
            Err(error) => status.error = Some(error.to_string()),
        }
        status
    }

    fn ensure_installed(&self, plugin_id: &PluginId) -> Result<(), PluginRuntimeError> {
        let path = self.install_path(plugin_id);
        let metadata = fs::symlink_metadata(&path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                PluginRuntimeError::NotInstalled(plugin_id.to_string())
            } else {
                io_error("inspect plugin installation", &path, source)
            }
        })?;
        if !metadata.file_type().is_dir() {
            return Err(PluginRuntimeError::NotInstalled(plugin_id.to_string()));
        }
        Ok(())
    }

    fn plugin_id(&self, value: &str) -> Result<PluginId, PluginRuntimeError> {
        PluginId::new(value).map_err(|error| PluginRuntimeError::Manifest {
            path: self.paths.install_dir.join(value).join(MANIFEST_FILENAME),
            source: ManifestError::InvalidPluginId(error),
        })
    }

    fn install_path(&self, plugin_id: &PluginId) -> PathBuf {
        self.paths.install_dir.join(plugin_id.as_str())
    }

    fn state_path(&self, plugin_id: &PluginId) -> PathBuf {
        self.paths
            .state_dir
            .join(format!("{}.toml", plugin_id.as_str()))
    }
}

fn read_manifest(path: &Path) -> Result<PluginManifest, ManifestError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| ManifestError::Read {
        path: path.to_owned(),
        reason: source.to_string(),
    })?;
    if !metadata.file_type().is_file() {
        return Err(ManifestError::NotRegular(path.to_owned()));
    }
    if metadata.len() > MAX_MANIFEST_BYTES as u64 {
        return Err(ManifestError::FileTooLarge {
            path: path.to_owned(),
            max: MAX_MANIFEST_BYTES,
        });
    }
    let source = fs::read_to_string(path).map_err(|error| ManifestError::Read {
        path: path.to_owned(),
        reason: error.to_string(),
    })?;
    validate_manifest(&source)
}

fn read_limited_regular_file(path: &Path, max: u64) -> Result<Vec<u8>, PluginRuntimeError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| io_error("inspect", path, source))?;
    if !metadata.file_type().is_file() {
        return Err(PluginRuntimeError::Io {
            action: "read regular file",
            path: path.to_owned(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "not a regular file"),
        });
    }
    if metadata.len() > max {
        return Err(PluginRuntimeError::Io {
            action: "read bounded file",
            path: path.to_owned(),
            source: io::Error::new(io::ErrorKind::InvalidData, "file exceeds size limit"),
        });
    }
    fs::read(path).map_err(|source| io_error("read", path, source))
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), PluginRuntimeError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| io_error("create", path, source))?;
    file.write_all(bytes)
        .map_err(|source| io_error("write", path, source))?;
    file.sync_all()
        .map_err(|source| io_error("sync", path, source))
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> PluginRuntimeError {
    PluginRuntimeError::Io {
        action,
        path: path.to_owned(),
        source,
    }
}
