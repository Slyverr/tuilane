//! Host-managed installation, permission checks, and Wasmtime execution for plugins.

mod component;
mod manager;
mod manifest;
mod state;

pub use component::{HostServiceDispatcher, HostServiceError};
pub use manager::{PluginManager, PluginPaths, PluginRuntimeError, PluginStatus, RuntimeView};
pub use manifest::{ManifestError, validate_manifest};
pub use state::PluginState;
