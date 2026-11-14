use std::{cell::RefCell, path::PathBuf, rc::Rc};

use clap::Subcommand;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use serde::Deserialize;
use tuilane_core::{CommandCallerId, CommandDispatcher};
use tuilane_plugin_api::{
    CommandPermission, ViewDescriptor, ViewId,
    plugin::{
        PLUGIN_API_VERSION, PluginId, PluginViewModel, WORKSPACE_INFO_CAPABILITY,
        WORKSPACE_SEARCH_CAPABILITY,
    },
    search::{SearchCommand, SearchCommandResult, SearchQuery},
};
use tuilane_plugin_runtime::{
    HostServiceDispatcher, HostServiceError, PluginManager, PluginPaths, RuntimeView,
};
use tuilane_ui::{View, ViewAction};

#[derive(Debug, Subcommand)]
pub(crate) enum PluginCliCommand {
    /// List installed external plugins and compatibility errors.
    List,
    /// List the manifest-declared commands for an installed plugin.
    Commands { id: String },
    /// Show requested host services and their current permission grants.
    Permissions { id: String },
    /// Install a plugin bundle directory containing manifest.toml and plugin.wasm.
    Install { bundle: PathBuf },
    /// Enable an installed plugin.
    Enable { id: String },
    /// Disable an installed plugin while preserving its grants and data.
    Disable { id: String },
    /// Grant a permission requested by an installed plugin.
    Grant { id: String, permission: String },
    /// Revoke a permission from an installed plugin.
    Revoke { id: String, permission: String },
    /// Uninstall a plugin while preserving its application data.
    Uninstall { id: String },
    /// Invoke one manifest-declared plugin command with a JSON request.
    Call {
        id: String,
        command: String,
        /// JSON request passed to the plugin command.
        #[arg(long, default_value = "{}")]
        request: String,
    },
}

pub(crate) fn create_manager(
    commands: Rc<RefCell<CommandDispatcher>>,
    data_dir: PathBuf,
) -> PluginManager {
    let paths = PluginPaths::new(
        data_dir.join("plugins/installed"),
        data_dir.join("plugins/state"),
        data_dir.join("plugins/data"),
    );
    let host_services: Rc<RefCell<dyn HostServiceDispatcher>> =
        Rc::new(RefCell::new(ApplicationHostServices { commands }));
    PluginManager::new(paths, host_services)
}

struct ApplicationHostServices {
    commands: Rc<RefCell<CommandDispatcher>>,
}

impl HostServiceDispatcher for ApplicationHostServices {
    fn supports(&self, capability: &str) -> bool {
        matches!(
            capability,
            WORKSPACE_INFO_CAPABILITY | WORKSPACE_SEARCH_CAPABILITY
        )
    }

    fn call(
        &mut self,
        plugin_id: &PluginId,
        capability: &str,
        request: &str,
    ) -> Result<String, HostServiceError> {
        match capability {
            WORKSPACE_INFO_CAPABILITY => {
                let request: serde_json::Value =
                    serde_json::from_str(request).map_err(|_| HostServiceError::InvalidRequest)?;
                if request.as_object().is_none_or(|object| !object.is_empty()) {
                    return Err(HostServiceError::InvalidRequest);
                }
                serde_json::to_string(&serde_json::json!({
                    "name": "tuilane",
                    "version": env!("CARGO_PKG_VERSION"),
                    "plugin_api_version": PLUGIN_API_VERSION,
                }))
                .map_err(|_| HostServiceError::Failed)
            }
            WORKSPACE_SEARCH_CAPABILITY => self.search(plugin_id, request),
            _ => Err(HostServiceError::InvalidRequest),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchRequest {
    query: String,
    kind: Option<String>,
}

impl ApplicationHostServices {
    fn search(&mut self, plugin_id: &PluginId, request: &str) -> Result<String, HostServiceError> {
        let request: SearchRequest =
            serde_json::from_str(request).map_err(|_| HostServiceError::InvalidRequest)?;
        let query =
            SearchQuery::new(request.query).map_err(|_| HostServiceError::InvalidRequest)?;
        let command = match request.kind {
            Some(kind) => SearchCommand::new(query)
                .with_kind(kind)
                .map_err(|_| HostServiceError::InvalidRequest)?,
            None => SearchCommand::new(query),
        };
        let caller =
            CommandCallerId::new(plugin_id.as_str()).map_err(|_| HostServiceError::Failed)?;
        let result = {
            let mut commands = self.commands.borrow_mut();
            let permission = CommandPermission::new("search.read");
            commands.grant_permission(&caller, permission);
            let execution = commands.execute_as(&caller, command);
            commands.revoke_permission(&caller, permission);
            execution
                .into_result()
                .map_err(|_| HostServiceError::Failed)?
        };
        let SearchCommandResult::Results(results) = result;
        let results = results
            .iter()
            .map(|result| {
                serde_json::json!({
                    "reference": result.item().reference().to_string(),
                    "kind": result.item().reference().kind(),
                    "id": result.item().reference().id(),
                    "title": result.item().title(),
                    "excerpt": result.excerpt(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&serde_json::json!({ "results": results }))
            .map_err(|_| HostServiceError::Failed)
    }
}

pub(crate) struct ExternalPluginView {
    manager: Rc<RefCell<PluginManager>>,
    plugin_id: PluginId,
    view_id: String,
    descriptor: ViewDescriptor,
    model: Option<PluginViewModel>,
    failure: Option<String>,
}

impl ExternalPluginView {
    pub(crate) fn new(
        contribution: RuntimeView,
        manager: Rc<RefCell<PluginManager>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let view = contribution.view;
        let descriptor = ViewDescriptor::new(
            ViewId::new(&view.id)?,
            view.title.clone(),
            view.description.clone(),
        );
        Ok(Self {
            manager,
            plugin_id: contribution.plugin_id,
            view_id: view.id,
            descriptor,
            model: None,
            failure: None,
        })
    }

    fn activate_action(&mut self, action_id: &str) -> bool {
        let Some(action_id) = self
            .model
            .as_ref()
            .and_then(|model| {
                model
                    .actions()
                    .iter()
                    .find(|action| action.id() == action_id)
            })
            .map(|action| action.id().to_owned())
        else {
            return false;
        };
        match self.manager.borrow_mut().activate_view(
            self.plugin_id.as_str(),
            &self.view_id,
            &action_id,
            "{}",
        ) {
            Ok(model) => self.model = Some(model),
            Err(error) => {
                self.model = None;
                self.failure = Some(error.to_string());
            }
        }
        true
    }
}

impl View for ExternalPluginView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn navigation_icon(&self) -> &'static str {
        "◇"
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if self.model.is_none() && self.failure.is_none() {
            match self
                .manager
                .borrow_mut()
                .render_view(self.plugin_id.as_str(), &self.view_id)
            {
                Ok(model) => self.model = Some(model),
                Err(error) => self.failure = Some(error.to_string()),
            }
        }

        let block = Block::default().borders(Borders::ALL);
        if let Some(error) = &self.failure {
            frame.render_widget(
                Paragraph::new(format!("Plugin view failed: {error}"))
                    .block(block.title(self.descriptor.title()))
                    .wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
        let Some(model) = &self.model else {
            frame.render_widget(
                Paragraph::new("Plugin view is not available.").block(block),
                area,
            );
            return;
        };

        let mut lines = model
            .lines()
            .iter()
            .map(|line| Line::raw(line.as_ref()))
            .collect::<Vec<_>>();
        if !model.actions().is_empty() {
            lines.push(Line::raw(""));
            lines.extend(
                model
                    .actions()
                    .iter()
                    .enumerate()
                    .map(|(index, action)| Line::raw(format!("{}  {}", index + 1, action.title()))),
            );
        }
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(block.title(model.title()))
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn command_palette_actions(&self) -> Vec<ViewAction> {
        self.model
            .as_ref()
            .into_iter()
            .flat_map(|model| model.actions())
            .map(|action| ViewAction::new(action.id(), action.title(), action.description()))
            .collect()
    }

    fn activate_command_palette_action(&mut self, id: &str) -> bool {
        self.activate_action(id)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    #[ignore = "run by scripts/check-plugin-sdk.sh with the built example component"]
    fn external_plugin_view_renders_and_activates_host_palette_actions() {
        let bundle = PathBuf::from(
            std::env::var_os("TUILANE_PLUGIN_TEST_BUNDLE")
                .expect("the plugin SDK validation script supplies a built bundle"),
        );
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        let data_dir = std::env::temp_dir().join(format!(
            "tuilane-plugin-view-test-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir(&data_dir).expect("create isolated plugin data directory");

        let commands = Rc::new(RefCell::new(CommandDispatcher::new()));
        let mut plugin_manager = create_manager(commands, data_dir.clone());
        plugin_manager
            .install(bundle)
            .expect("install sample plugin");
        plugin_manager
            .enable("org.example.hello")
            .expect("enable sample plugin");
        plugin_manager
            .grant("org.example.hello", "workspace.read")
            .expect("grant workspace information service");

        let plugin_manager = Rc::new(RefCell::new(plugin_manager));
        let contribution = plugin_manager
            .borrow()
            .views()
            .into_iter()
            .next()
            .expect("sample plugin provides a view");
        let mut view = ExternalPluginView::new(contribution, plugin_manager.clone())
            .expect("create host view adapter");

        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("create test terminal");
        terminal
            .draw(|frame| view.render(frame, frame.area()))
            .expect("render plugin view");
        let output = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(output.contains("Connected to tuilane"), "{output}");
        assert_eq!(view.command_palette_actions().len(), 1);
        assert!(view.activate_command_palette_action("refresh"));

        drop(view);
        drop(plugin_manager);
        fs::remove_dir_all(data_dir).expect("remove isolated plugin data directory");
    }
}
