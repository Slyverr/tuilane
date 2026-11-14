# External plugin SDK (beta)

The external plugin SDK is an early, versioned WebAssembly component interface. It lets an independently built plugin declare commands and contribute a text-based workspace view, then request supported services through the host. The API is experimental and may change before a future stability commitment.

The current interface is defined in [`crates/tuilane-plugin-api/wit/plugin.wit`](../crates/tuilane-plugin-api/wit/plugin.wit). Components use the `plugin-guest` world and report API version `0.1.0`. A bundle contains exactly the following required files; installation ignores other files in the bundle:

```text
hello-world-bundle/
├── manifest.toml
└── plugin.wasm
```

The manifest declares a namespaced plugin ID, plugin and API versions, requested host services and permissions, commands, and views. The guest's exported metadata and contributions must match the manifest. Invalid manifests, missing imports, incompatible WIT exports, and unsupported host services are reported as compatibility errors by `tuilane plugin list` or the operation that loads the plugin.

## Build and exercise the sample

[`examples/plugins/hello-world`](../examples/plugins/hello-world) is a separate Cargo project. It depends on the public WIT contract and does not link tuilane application crates into the guest. Run the SDK validation from the repository root in an environment with the stable `wasm32-unknown-unknown` Rust target and `wasm-tools`:

```sh
nix develop .#runtime-evaluation -c scripts/check-plugin-sdk.sh
```

The script builds the guest, converts it into a Wasm component, runs lifecycle and permission tests, then exercises the CLI using temporary application data. It verifies installation, denied access before a grant, service calls after a grant, revocation, disablement, and uninstall data retention.

To try the generated bundle manually, first build it with the same command, then use:

```sh
tuilane plugin install target/plugin-sdk/hello-world-bundle
tuilane plugin enable org.example.hello
tuilane plugin commands org.example.hello
tuilane plugin permissions org.example.hello
tuilane plugin grant org.example.hello workspace.read
tuilane plugin call org.example.hello org.example.hello.greet@1 --request '{"name":"Ada"}'
tuilane plugin grant org.example.hello workspace.search
tuilane plugin call org.example.hello org.example.hello.search@1 --request '{"query":"planning"}'
tuilane plugin revoke org.example.hello workspace.read
tuilane plugin disable org.example.hello
tuilane plugin uninstall org.example.hello
```

Installation leaves a plugin disabled until explicitly enabled. The host can list a plugin's manifest-declared commands and requested services with their grant status; `plugin call` invokes one declared versioned command through the runtime. Grants are persisted per plugin and checked for every host service request, so revocation takes effect on the next request. Uninstall removes the bundle and grants while preserving the host-owned plugin data directory. This release does not yet expose a plugin data API.

## Host boundary and current limits

The host currently implements two read-only services: `workspace.info@1` and `workspace.search@1`. `workspace.info@1` returns the application and plugin API versions. `workspace.search@1` validates its JSON request and uses the same typed, permission-aware command dispatcher as built-in interfaces. The plugin receives only the search results and metadata returned by that service.

The guest has no WASI linker, filesystem access, network access, or direct database access. The runtime limits component size to 64 MiB, linear memory to 32 MiB, guest initialization and each guest call to 10 million fuel units, each guest call to a 250 ms epoch deadline, JSON RPC payloads to 1 MiB, and host-service calls to 64 per guest call. A failed or interrupted guest instance is discarded before another call. These controls describe this implementation; they are not a claim that every hostile component or platform has been independently security-audited.

The host renders bounded text and action descriptions returned by the plugin. Plugins do not control the terminal lifecycle, layout, or event loop. Plugin command effects are descriptive metadata; the currently exposed host services are read-only, and there is no general plugin storage, mutation, marketplace, automatic updater, or hot-reload facility.

The interface is not yet a stable third-party compatibility guarantee. Breaking changes may be made during `0.x` development and must be documented. See [architecture](architecture.md) for crate ownership and [ROADMAP.md](../ROADMAP.md) for the milestone plan.
