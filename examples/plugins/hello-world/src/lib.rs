wit_bindgen::generate!({
    path: "../../../crates/tuilane-plugin-api/wit",
    world: "plugin-guest",
});

struct HelloWorld;

impl exports::tuilane::plugin::plugin::Guest for HelloWorld {
    fn metadata() -> exports::tuilane::plugin::plugin::PluginMetadata {
        exports::tuilane::plugin::plugin::PluginMetadata {
            id: "org.example.hello".to_owned(),
            api_version: "0.1.0".to_owned(),
        }
    }

    fn commands() -> Vec<exports::tuilane::plugin::plugin::Command> {
        use exports::tuilane::plugin::plugin::{Command, Effect};

        vec![
            Command {
                id: "org.example.hello.greet@1".to_owned(),
                summary: "Return a greeting and workspace version.".to_owned(),
                effect: Effect::ReadOnly,
            },
            Command {
                id: "org.example.hello.search@1".to_owned(),
                summary: "Search workspace items through the host.".to_owned(),
                effect: Effect::ReadOnly,
            },
        ]
    }

    fn views() -> Vec<exports::tuilane::plugin::plugin::View> {
        vec![exports::tuilane::plugin::plugin::View {
            id: "org.example.hello.main".to_owned(),
            title: "Hello World".to_owned(),
            description: "An independently built example plugin view.".to_owned(),
        }]
    }

    fn invoke(
        command_id: String,
        request: String,
    ) -> Result<String, exports::tuilane::plugin::plugin::PluginError> {
        use exports::tuilane::plugin::plugin::PluginError;

        let request: serde_json::Value =
            serde_json::from_str(&request).map_err(|_| PluginError::InvalidRequest)?;
        match command_id.as_str() {
            "org.example.hello.greet@1" => {
                let name = request
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("world");
                let info = tuilane::plugin::host::request("workspace.info@1", "{}")
                    .map_err(map_host_error)?;
                let info: serde_json::Value =
                    serde_json::from_str(&info).map_err(|_| PluginError::Failed)?;
                Ok(serde_json::json!({
                    "message": format!("Hello, {name}!"),
                    "workspace": info,
                })
                .to_string())
            }
            "org.example.hello.search@1" => {
                let query = request
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(PluginError::InvalidRequest)?;
                let mut host_request = serde_json::json!({ "query": query });
                if let Some(kind) = request.get("kind").and_then(serde_json::Value::as_str) {
                    host_request["kind"] = serde_json::Value::String(kind.to_owned());
                }
                tuilane::plugin::host::request("workspace.search@1", &host_request.to_string())
                    .map_err(map_host_error)
            }
            _ => Err(PluginError::UnknownCommand),
        }
    }

    fn render(
        view_id: String,
    ) -> Result<
        exports::tuilane::plugin::plugin::ViewModel,
        exports::tuilane::plugin::plugin::PluginError,
    > {
        if view_id != "org.example.hello.main" {
            return Err(exports::tuilane::plugin::plugin::PluginError::UnknownView);
        }
        view_model()
    }

    fn activate(
        view_id: String,
        action_id: String,
        request: String,
    ) -> Result<
        exports::tuilane::plugin::plugin::ViewModel,
        exports::tuilane::plugin::plugin::PluginError,
    > {
        if view_id != "org.example.hello.main" {
            return Err(exports::tuilane::plugin::plugin::PluginError::UnknownView);
        }
        if action_id != "refresh" {
            return Err(exports::tuilane::plugin::plugin::PluginError::UnknownAction);
        }
        serde_json::from_str::<serde_json::Value>(&request)
            .map_err(|_| exports::tuilane::plugin::plugin::PluginError::InvalidRequest)?;
        view_model()
    }
}

fn view_model() -> Result<
    exports::tuilane::plugin::plugin::ViewModel,
    exports::tuilane::plugin::plugin::PluginError,
> {
    use exports::tuilane::plugin::plugin::{Action, PluginError, ViewModel};

    let info = tuilane::plugin::host::request("workspace.info@1", "{}").map_err(map_host_error)?;
    let info: serde_json::Value = serde_json::from_str(&info).map_err(|_| PluginError::Failed)?;
    let version = info
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    Ok(ViewModel {
        title: "Hello World".to_owned(),
        lines: vec![
            "This view runs in an isolated WebAssembly component.".to_owned(),
            format!("Connected to tuilane {version} through workspace.info@1."),
            "The host draws this bounded text model and owns the terminal.".to_owned(),
        ],
        actions: vec![Action {
            id: "refresh".to_owned(),
            title: "Refresh workspace information".to_owned(),
            description: "Call the host workspace.info@1 service again.".to_owned(),
        }],
    })
}

fn map_host_error(
    error: tuilane::plugin::host::HostError,
) -> exports::tuilane::plugin::plugin::PluginError {
    use exports::tuilane::plugin::plugin::PluginError;

    match error {
        tuilane::plugin::host::HostError::Denied => PluginError::PermissionDenied,
        tuilane::plugin::host::HostError::InvalidRequest => PluginError::InvalidRequest,
        _ => PluginError::Failed,
    }
}

export!(HelloWorld);
