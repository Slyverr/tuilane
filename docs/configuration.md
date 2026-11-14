# Configuration

tuilane reads an optional TOML configuration file. If the file does not exist, built-in defaults are used. The default location follows the platform's application configuration directory. Set `TUILANE_CONFIG` to use another file, or pass a path to `config check`.

```toml
version = 1

[ui]
theme = "default" # default or high-contrast
layout = "balanced" # balanced or focus
keymap = "default" # default or vim; both support arrows and j/k for view navigation
```

The `balanced` layout shows navigation when terminal width allows it; `focus` hides the navigation panel. The `high-contrast` theme uses brighter shell text and yellow accents. Use `j` to move down to the next view and `k` to move up to the previous view. Arrow keys move within a focused view list; when focus is on the Views panel, Up/Down also switch views. Text inputs can still accept `j` and `k` while editing. Both accepted keymap values use the same view-navigation keys.

## Optional local assistant

The assistant is disabled by default. To enable it, add a local Ollama-compatible model endpoint to the configuration:

```toml
[assistant]
enabled = true
model = "your-tool-capable-model"
endpoint = "http://127.0.0.1:11434"
```

Replace the model name with one installed locally that supports tool calling, and make sure the service is running. See [Ollama's tool-calling guide](https://docs.ollama.com/capabilities/tool-calling) for provider setup. The endpoint must be plain HTTP on a loopback address; remote hosts, credentials, paths, queries, redirects, and proxies are not accepted. Prompts and bounded open-task summaries are sent to that local service. Chat history is not saved. The assistant can read open tasks and propose task creation or completion; each task change shows a preview and requires approval. Deletion is unavailable. Press `Esc` to cancel a response, `d` to show the approval drawer, `y` to approve, `n` or `Esc` to reject, and `Ctrl-L` to clear the conversation.

Validate the selected configuration without starting the TUI:

```sh
cargo run -p tuilane -- config check
cargo run -p tuilane -- config check ./tuilane.toml
```

The checker reports parse errors, unsupported versions, unknown fields, and unsupported values with their configuration path. Assistant settings are the only plugin-specific configuration currently supported; other plugin settings and interactive configuration editing are not implemented.
