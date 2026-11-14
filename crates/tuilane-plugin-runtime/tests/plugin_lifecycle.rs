use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use tuilane_plugin_api::plugin::{
    PluginId, WORKSPACE_INFO_CAPABILITY, WORKSPACE_SEARCH_CAPABILITY,
};
use tuilane_plugin_runtime::{
    HostServiceDispatcher, HostServiceError, PluginManager, PluginPaths, PluginRuntimeError,
};

const PLUGIN_ID: &str = "org.example.hello";

#[test]
#[ignore = "run by scripts/check-plugin-sdk.sh with the built example component"]
fn installed_plugin_observes_grants_revocation_unload_and_data_retention() {
    let source_bundle = PathBuf::from(
        std::env::var_os("TUILANE_PLUGIN_TEST_BUNDLE")
            .expect("the plugin SDK validation script supplies a built bundle"),
    );
    let temporary = TestDirectory::new();
    let bundle = temporary.path().join("bundle");
    fs::create_dir(&bundle).expect("create package bundle");
    fs::copy(
        source_bundle.join("manifest.toml"),
        bundle.join("manifest.toml"),
    )
    .expect("copy plugin manifest");
    fs::copy(
        source_bundle.join("plugin.wasm"),
        bundle.join("plugin.wasm"),
    )
    .expect("copy component");
    fs::write(bundle.join("unmanaged.txt"), "not installed").expect("write unrelated bundle file");

    let paths = PluginPaths::new(
        temporary.path().join("installed"),
        temporary.path().join("state"),
        temporary.path().join("data"),
    );
    let service_state = Rc::new(RefCell::new(TestHostServices::default()));
    let services: Rc<RefCell<dyn HostServiceDispatcher>> = service_state.clone();
    let mut manager = PluginManager::new(paths.clone(), services);

    let invalid_bundle = temporary.path().join("invalid-bundle");
    fs::create_dir(&invalid_bundle).expect("create invalid package");
    fs::write(
        invalid_bundle.join("manifest.toml"),
        r#"
manifest-version = 1
id = "org.example.broken"
name = "Broken Component"
version = "0.1.0"
api-version = "0.1.0"
component = "plugin.wasm"
"#,
    )
    .expect("write valid manifest for invalid component");
    fs::write(invalid_bundle.join("plugin.wasm"), b"not a component")
        .expect("write invalid component bytes");
    assert!(matches!(
        manager.install(&invalid_bundle),
        Err(PluginRuntimeError::Component(_))
    ));
    assert!(!paths.install_dir.join("org.example.broken").exists());
    assert!(!paths.state_dir.join("org.example.broken.toml").exists());

    let installed = manager.install(&bundle).expect("install compatible plugin");
    assert_eq!(installed.as_str(), PLUGIN_ID);
    assert!(matches!(
        manager.install(&bundle),
        Err(PluginRuntimeError::AlreadyInstalled(id)) if id == PLUGIN_ID
    ));
    assert!(matches!(
        manager.grant(PLUGIN_ID, "workspace.admin"),
        Err(PluginRuntimeError::PermissionNotRequested { .. })
    ));
    assert!(
        !paths
            .install_dir
            .join(PLUGIN_ID)
            .join("unmanaged.txt")
            .exists()
    );

    manager.enable(PLUGIN_ID).expect("enable plugin");
    assert_eq!(manager.views().len(), 1);
    assert_eq!(manager.commands(PLUGIN_ID).unwrap().len(), 2);
    assert!(matches!(
        manager.invoke(PLUGIN_ID, "org.example.hello.greet@1", r#"{"name":"Ada"}"#),
        Err(PluginRuntimeError::Component(message)) if message.contains("permission denied")
    ));
    assert!(service_state.borrow().calls.is_empty());

    manager
        .grant(PLUGIN_ID, "workspace.read")
        .expect("grant workspace info");
    let response = manager
        .invoke(PLUGIN_ID, "org.example.hello.greet@1", r#"{"name":"Ada"}"#)
        .expect("invoke granted command");
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["message"], "Hello, Ada!");
    assert_eq!(response["workspace"]["name"], "test-host");

    let view = manager
        .render_view(PLUGIN_ID, "org.example.hello.main")
        .expect("render granted view");
    assert_eq!(view.actions().len(), 1);
    manager
        .activate_view(PLUGIN_ID, "org.example.hello.main", "refresh", "{}")
        .expect("activate declared view action");
    assert!(matches!(
        manager.activate_view(PLUGIN_ID, "org.example.hello.main", "not-listed", "{}"),
        Err(PluginRuntimeError::ActionUnavailable { .. })
    ));

    manager
        .grant(PLUGIN_ID, "workspace.search")
        .expect("grant workspace search");
    let response = manager
        .invoke(
            PLUGIN_ID,
            "org.example.hello.search@1",
            r#"{"query":"planning","kind":"task"}"#,
        )
        .expect("invoke host-backed search");
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["results"][0]["reference"], "task:example");
    assert!(service_state.borrow().calls.iter().any(|call| {
        call.capability == WORKSPACE_SEARCH_CAPABILITY
            && call.plugin_id == PLUGIN_ID
            && call.request.contains("planning")
    }));

    let info_calls_before_revoke = service_state
        .borrow()
        .calls
        .iter()
        .filter(|call| call.capability == WORKSPACE_INFO_CAPABILITY)
        .count();
    assert!(
        manager
            .revoke(PLUGIN_ID, "workspace.read")
            .expect("revoke workspace info")
    );
    assert!(matches!(
        manager.invoke(PLUGIN_ID, "org.example.hello.greet@1", r#"{"name":"Ada"}"#),
        Err(PluginRuntimeError::Component(message)) if message.contains("permission denied")
    ));
    assert_eq!(
        info_calls_before_revoke,
        service_state
            .borrow()
            .calls
            .iter()
            .filter(|call| call.capability == WORKSPACE_INFO_CAPABILITY)
            .count()
    );

    let data = paths.data_dir.join(PLUGIN_ID);
    fs::create_dir_all(&data).expect("create host-owned plugin data directory");
    let retained_file = data.join("user-data.txt");
    fs::write(&retained_file, "keep across uninstall").expect("write user data");

    manager.disable(PLUGIN_ID).expect("disable plugin");
    assert!(matches!(
        manager.invoke(
            PLUGIN_ID,
            "org.example.hello.greet@1",
            r#"{"name":"Ada"}"#
        ),
        Err(PluginRuntimeError::Disabled(id)) if id == PLUGIN_ID
    ));
    let status = manager.list().expect("list plugin").pop().unwrap();
    assert!(!status.enabled);
    assert_eq!(status.grants, ["workspace.search"]);

    manager.uninstall(PLUGIN_ID).expect("uninstall plugin");
    assert!(!paths.install_dir.join(PLUGIN_ID).exists());
    assert!(!paths.state_dir.join(format!("{PLUGIN_ID}.toml")).exists());
    assert_eq!(
        fs::read_to_string(retained_file).unwrap(),
        "keep across uninstall"
    );
}

#[derive(Default)]
struct TestHostServices {
    calls: Vec<ServiceCall>,
}

impl HostServiceDispatcher for TestHostServices {
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
        self.calls.push(ServiceCall {
            plugin_id: plugin_id.to_string(),
            capability: capability.to_owned(),
            request: request.to_owned(),
        });
        match capability {
            WORKSPACE_INFO_CAPABILITY => Ok(r#"{"name":"test-host","version":"1.0.0"}"#.to_owned()),
            WORKSPACE_SEARCH_CAPABILITY => Ok(
                r#"{"results":[{"reference":"task:example","kind":"task","id":"example","title":"Example","excerpt":"planning"}]}"#
                    .to_owned(),
            ),
            _ => Err(HostServiceError::InvalidRequest),
        }
    }
}

struct ServiceCall {
    plugin_id: String,
    capability: String,
    request: String,
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tuilane-plugin-runtime-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&path).expect("create temporary test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
