use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    thread,
    time::Duration,
};

use thiserror::Error;
use tuilane_plugin_api::plugin::{
    PLUGIN_API_VERSION, PluginAction, PluginEffect, PluginId, PluginManifest, PluginViewModel,
};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};

use crate::state;

pub(crate) mod bindings {
    wasmtime::component::bindgen!({
        path: "../tuilane-plugin-api/wit",
        world: "plugin-guest",
    });
}

pub(crate) const MAX_COMPONENT_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_RPC_BYTES: usize = 1024 * 1024;
const MAX_VIEW_BYTES: usize = 64 * 1024;
const MAX_VIEW_LINES: usize = 200;
const MAX_VIEW_ACTIONS: usize = 32;
const MAX_HOST_CALLS_PER_GUEST_CALL: usize = 64;
const MAX_FUEL_PER_GUEST_CALL: u64 = 10_000_000;
const MAX_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const MAX_CALL_TIME: Duration = Duration::from_millis(250);

/// Host-owned implementations of the explicitly supported plugin services.
pub trait HostServiceDispatcher {
    /// Returns whether the host implements the exact versioned capability.
    fn supports(&self, capability: &str) -> bool;

    /// Executes one validated JSON request on behalf of the named plugin.
    fn call(
        &mut self,
        plugin_id: &PluginId,
        capability: &str,
        request: &str,
    ) -> Result<String, HostServiceError>;
}

/// Error returned by an application-provided host service.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HostServiceError {
    /// The service request failed validation.
    #[error("invalid service request")]
    InvalidRequest,
    /// The service implementation failed without exposing private details.
    #[error("host service failed")]
    Failed,
}

#[derive(Debug, Error)]
pub(crate) enum ComponentError {
    #[error("could not inspect plugin component {path}: {source}")]
    Inspect {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("plugin component {path} exceeds {max} bytes")]
    ComponentTooLarge { path: PathBuf, max: u64 },
    #[error("could not read plugin component {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("plugin component could not be compiled: {0}")]
    Compile(String),
    #[error("plugin component does not match the host WIT contract: {0}")]
    IncompatibleComponent(String),
    #[error("plugin component could not be instantiated: {0}")]
    Instantiate(String),
    #[error("plugin guest call failed: {0}")]
    GuestCall(String),
    #[error("plugin rejected the host call: {0}")]
    PluginRejected(String),
    #[error("plugin host service permission denied")]
    PermissionDenied,
    #[error("plugin metadata does not match its manifest")]
    MetadataMismatch,
    #[error("plugin command contributions do not match its manifest")]
    CommandMismatch,
    #[error("plugin view contributions do not match its manifest")]
    ViewMismatch,
    #[error("plugin returned an invalid view model: {0}")]
    InvalidViewModel(&'static str),
    #[error("plugin returned an invalid JSON response")]
    InvalidJsonResponse,
    #[error("plugin request must be valid JSON within the configured size limit")]
    InvalidJsonRequest,
    #[error("could not configure the Wasm fuel limit: {0}")]
    Fuel(String),
    #[error("plugin execution watchdog could not join")]
    Watchdog,
}

pub(crate) fn create_engine() -> Result<Engine, ComponentError> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    Engine::new(&config).map_err(|error| ComponentError::Compile(error.to_string()))
}

pub(crate) struct PluginComponent {
    engine: Engine,
    store: Store<PluginStoreState>,
    guest: bindings::PluginGuest,
    manifest: PluginManifest,
}

impl PluginComponent {
    pub(crate) fn load(
        engine: &Engine,
        component_path: &Path,
        state_path: PathBuf,
        manifest: PluginManifest,
        host_services: Rc<RefCell<dyn HostServiceDispatcher>>,
    ) -> Result<Self, ComponentError> {
        let metadata = std::fs::symlink_metadata(component_path).map_err(|source| {
            ComponentError::Inspect {
                path: component_path.to_owned(),
                source,
            }
        })?;
        if !metadata.file_type().is_file() {
            return Err(ComponentError::IncompatibleComponent(
                "component must be a regular file".to_owned(),
            ));
        }
        if metadata.len() > MAX_COMPONENT_BYTES {
            return Err(ComponentError::ComponentTooLarge {
                path: component_path.to_owned(),
                max: MAX_COMPONENT_BYTES,
            });
        }
        if metadata.len() == 0 {
            return Err(ComponentError::IncompatibleComponent(
                "component file is empty".to_owned(),
            ));
        }
        let bytes = std::fs::read(component_path).map_err(|source| ComponentError::Read {
            path: component_path.to_owned(),
            source,
        })?;
        let component = Component::from_binary(engine, &bytes)
            .map_err(|error| ComponentError::Compile(error.to_string()))?;
        let plugin_id = PluginId::new(&manifest.id)
            .map_err(|error| ComponentError::IncompatibleComponent(error.to_string()))?;

        let state = PluginStoreState {
            plugin_id: plugin_id.clone(),
            manifest: manifest.clone(),
            state_path,
            host_services,
            limits: StoreLimitsBuilder::new()
                .memory_size(MAX_MEMORY_BYTES)
                .table_elements(10_000)
                .instances(10)
                .tables(10)
                .memories(1)
                .build(),
            remaining_host_calls: MAX_HOST_CALLS_PER_GUEST_CALL,
        };
        let mut linker = Linker::new(engine);
        bindings::PluginGuest::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(|error| ComponentError::IncompatibleComponent(error.to_string()))?;
        let mut store = Store::new(engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_epoch_deadline(1);
        store
            .set_fuel(MAX_FUEL_PER_GUEST_CALL)
            .map_err(|error| ComponentError::Fuel(error.to_string()))?;
        let guest = bindings::PluginGuest::instantiate(&mut store, &component, &linker)
            .map_err(|error| ComponentError::Instantiate(error.to_string()))?;

        let mut loaded = Self {
            engine: engine.clone(),
            store,
            guest,
            manifest,
        };
        loaded.validate_exports()?;
        Ok(loaded)
    }

    pub(crate) fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    pub(crate) fn render(&mut self, view_id: &str) -> Result<PluginViewModel, ComponentError> {
        let result = self.with_guest_call(|guest, store| guest.call_render(store, view_id))?;
        let model = result.map_err(plugin_rejection)?;
        validate_view_model(model)
    }

    pub(crate) fn activate(
        &mut self,
        view_id: &str,
        action_id: &str,
        request: &str,
    ) -> Result<PluginViewModel, ComponentError> {
        validate_json_request(request)?;
        let result = self.with_guest_call(|guest, store| {
            guest.call_activate(store, view_id, action_id, request)
        })?;
        let model = result.map_err(plugin_rejection)?;
        validate_view_model(model)
    }

    pub(crate) fn invoke(
        &mut self,
        command_id: &str,
        request: &str,
    ) -> Result<String, ComponentError> {
        validate_json_request(request)?;
        let result =
            self.with_guest_call(|guest, store| guest.call_invoke(store, command_id, request))?;
        let response = result.map_err(plugin_rejection)?;
        if response.len() > MAX_RPC_BYTES
            || serde_json::from_str::<serde_json::Value>(&response).is_err()
        {
            return Err(ComponentError::InvalidJsonResponse);
        }
        Ok(response)
    }

    fn validate_exports(&mut self) -> Result<(), ComponentError> {
        let metadata = self.with_guest_call(|guest, store| guest.call_metadata(store))?;
        if metadata.id != self.manifest.id || metadata.api_version != PLUGIN_API_VERSION {
            return Err(ComponentError::MetadataMismatch);
        }

        let commands = self.with_guest_call(|guest, store| guest.call_commands(store))?;
        if commands.len() != self.manifest.commands.len()
            || commands
                .iter()
                .zip(&self.manifest.commands)
                .any(|(guest, manifest)| {
                    guest.id != manifest.id
                        || guest.summary != manifest.summary
                        || effect_from_wit(guest.effect) != manifest.effect
                })
        {
            return Err(ComponentError::CommandMismatch);
        }

        let views = self.with_guest_call(|guest, store| guest.call_views(store))?;
        if views.len() != self.manifest.views.len()
            || views
                .iter()
                .zip(&self.manifest.views)
                .any(|(guest, manifest)| {
                    guest.id != manifest.id
                        || guest.title != manifest.title
                        || guest.description != manifest.description
                })
        {
            return Err(ComponentError::ViewMismatch);
        }
        Ok(())
    }

    fn with_guest_call<T>(
        &mut self,
        call: impl FnOnce(
            &bindings::exports::tuilane::plugin::plugin::Guest,
            &mut Store<PluginStoreState>,
        ) -> Result<T, wasmtime::Error>,
    ) -> Result<T, ComponentError> {
        self.store
            .set_fuel(MAX_FUEL_PER_GUEST_CALL)
            .map_err(|error| ComponentError::Fuel(error.to_string()))?;
        self.store.set_epoch_deadline(1);
        self.store.data_mut().remaining_host_calls = MAX_HOST_CALLS_PER_GUEST_CALL;

        let engine = self.engine.clone();
        let (stop, wait) = mpsc::channel();
        let watchdog = thread::spawn(move || {
            if wait.recv_timeout(MAX_CALL_TIME).is_err() {
                engine.increment_epoch();
            }
        });
        let result = call(self.guest.tuilane_plugin_plugin(), &mut self.store)
            .map_err(|error| ComponentError::GuestCall(error.to_string()));
        let _ = stop.send(());
        watchdog.join().map_err(|_| ComponentError::Watchdog)?;
        result
    }
}

fn plugin_rejection(
    error: bindings::exports::tuilane::plugin::plugin::PluginError,
) -> ComponentError {
    use bindings::exports::tuilane::plugin::plugin::PluginError;

    match error {
        PluginError::PermissionDenied => ComponentError::PermissionDenied,
        error => ComponentError::PluginRejected(format!("{error:?}")),
    }
}

struct PluginStoreState {
    plugin_id: PluginId,
    manifest: PluginManifest,
    state_path: PathBuf,
    host_services: Rc<RefCell<dyn HostServiceDispatcher>>,
    limits: StoreLimits,
    remaining_host_calls: usize,
}

impl bindings::tuilane::plugin::host::Host for PluginStoreState {
    fn request(
        &mut self,
        capability: String,
        request: String,
    ) -> Result<String, bindings::tuilane::plugin::host::HostError> {
        use bindings::tuilane::plugin::host::HostError;

        if capability.len() > 192 || request.len() > MAX_RPC_BYTES || !valid_json(&request) {
            return Err(HostError::InvalidRequest);
        }
        if self.remaining_host_calls == 0 {
            return Err(HostError::Failed);
        }
        self.remaining_host_calls -= 1;

        let requested = self
            .manifest
            .services
            .iter()
            .find(|service| service.id == capability)
            .ok_or(HostError::UnknownCapability)?;
        let state = state::read(&self.state_path).map_err(|_| HostError::Denied)?;
        if !state.is_enabled() || !state.has_permission(&requested.permission) {
            return Err(HostError::Denied);
        }

        let services = self.host_services.clone();
        if !services.borrow().supports(&capability) {
            return Err(HostError::UnknownCapability);
        }
        let response = services
            .borrow_mut()
            .call(&self.plugin_id, &capability, &request)
            .map_err(|error| match error {
                HostServiceError::InvalidRequest => HostError::InvalidRequest,
                HostServiceError::Failed => HostError::Failed,
            })?;
        if response.len() > MAX_RPC_BYTES || !valid_json(&response) {
            return Err(HostError::Failed);
        }
        Ok(response)
    }
}

fn validate_json_request(request: &str) -> Result<(), ComponentError> {
    if request.len() > MAX_RPC_BYTES || !valid_json(request) {
        return Err(ComponentError::InvalidJsonRequest);
    }
    Ok(())
}

fn valid_json(value: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(value).is_ok()
}

fn effect_from_wit(effect: bindings::exports::tuilane::plugin::plugin::Effect) -> PluginEffect {
    use bindings::exports::tuilane::plugin::plugin::Effect;

    match effect {
        Effect::ReadOnly => PluginEffect::ReadOnly,
        Effect::Mutating => PluginEffect::Mutating,
        Effect::Destructive => PluginEffect::Destructive,
    }
}

fn validate_view_model(
    model: bindings::exports::tuilane::plugin::plugin::ViewModel,
) -> Result<PluginViewModel, ComponentError> {
    let total_bytes = model.title.len()
        + model.lines.iter().map(String::len).sum::<usize>()
        + model
            .actions
            .iter()
            .map(|action| action.id.len() + action.title.len() + action.description.len())
            .sum::<usize>();
    if total_bytes > MAX_VIEW_BYTES {
        return Err(ComponentError::InvalidViewModel(
            "exceeds the text size limit",
        ));
    }
    if model.title.trim().is_empty()
        || model.title.chars().count() > 80
        || model.title.chars().any(char::is_control)
    {
        return Err(ComponentError::InvalidViewModel(
            "contains an invalid title",
        ));
    }
    if model.lines.len() > MAX_VIEW_LINES
        || model
            .lines
            .iter()
            .any(|line| line.chars().count() > 512 || line.chars().any(char::is_control))
    {
        return Err(ComponentError::InvalidViewModel(
            "contains invalid display lines",
        ));
    }
    if model.actions.len() > MAX_VIEW_ACTIONS {
        return Err(ComponentError::InvalidViewModel(
            "contains too many actions",
        ));
    }

    let mut ids = std::collections::HashSet::new();
    let mut actions = Vec::with_capacity(model.actions.len());
    for action in model.actions {
        if !valid_local_id(&action.id)
            || !ids.insert(action.id.clone())
            || action.title.trim().is_empty()
            || action.title.chars().count() > 80
            || action.title.chars().any(char::is_control)
            || action.description.chars().count() > 256
            || action.description.chars().any(char::is_control)
        {
            return Err(ComponentError::InvalidViewModel(
                "contains an invalid action",
            ));
        }
        actions.push(PluginAction::new(
            action.id,
            action.title,
            action.description,
        ));
    }

    Ok(PluginViewModel::new(
        model.title,
        model.lines.into_iter().map(Into::into).collect(),
        actions,
    ))
}

fn valid_local_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}
