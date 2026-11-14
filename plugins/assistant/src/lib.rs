//! Opt-in local assistant view and its host-mediated task tools.

mod config;
mod provider;
mod view;

pub use config::{AssistantConfig, AssistantConfigError, DEFAULT_ENDPOINT};
pub use view::AssistantView;
