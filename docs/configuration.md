# Configuration

tuilane reads an optional TOML configuration file. If the file does not exist, built-in defaults are used. The default location follows the platform's application configuration directory. Set `TUILANE_CONFIG` to use another file, or pass a path to `config check`.

```toml
version = 1

[ui]
theme = "default" # default or high-contrast
layout = "balanced" # balanced or focus
keymap = "default" # default or vim; both currently use the same bindings
```

The `balanced` layout shows the contextual side panel when terminal width allows it; `focus` keeps the main view full width. The top tab row remains available in both layouts. The `high-contrast` theme uses brighter shell text and yellow accents. Press `Shift+H` or `Shift+L` to switch to the previous or next view; the new view activates immediately. Press `Alt+Tab` to open the view picker, then use `Alt+Tab`, `j`/`k`, or Up/Down to choose a view, `Enter` to activate it, or `Esc` to close the picker. Press `Tab` to focus the tab row; `j`/`k` and Up/Down switch views immediately, while `Enter`, `l`, or Left Arrow returns focus to the active view. In a view, `j`/`k` or Up/Down move through lists and calendar weeks; active text inputs still receive typed characters. Press `Tab` to move focus between the tabs, content, and an open side panel. `i` toggles contextual details and `d` toggles supplemental content in the same right-side panel. With a mouse, click a tab or scroll over the tabs to activate a view, and click a visible panel to focus it. The one-line footer shows higher-priority view and workspace shortcuts; press `?` to see all available bindings. The `default` and `vim` keymap names are accepted for configuration compatibility and currently use the same bindings.

## Optional local assistant

The assistant is disabled by default. To enable it, add a local Ollama-compatible model endpoint to the configuration:

```toml
[assistant]
enabled = true
model = "your-tool-capable-model"
endpoint = "http://127.0.0.1:11434"
```

Replace the model name with one installed locally that supports tool calling, and make sure the service is running. See [Ollama's tool-calling guide](https://docs.ollama.com/capabilities/tool-calling) for provider setup. The endpoint must be plain HTTP on a loopback address; remote hosts, credentials, paths, queries, redirects, and proxies are not accepted. Prompts and bounded open-task summaries are sent to that local service. Chat history is not saved. The assistant can read open tasks and propose task creation or completion; each task change shows a preview and requires approval. Deletion is unavailable. Press `Esc` to cancel a response, `d` to show the assistant side panel, `y` to approve, `n` or `Esc` to reject, and `Ctrl-L` to clear the conversation.

Validate the selected configuration without starting the TUI:

```sh
cargo run -p tuilane -- config check
cargo run -p tuilane -- config check ./tuilane.toml
```

The checker reports parse errors, unsupported versions, unknown fields, and unsupported values with their configuration path. Assistant settings are the only plugin-specific configuration currently supported; other plugin settings and interactive configuration editing are not implemented.
