# Configuration

tuilane reads an optional TOML configuration file. If the file does not exist, built-in defaults are used. The default location follows the platform's application configuration directory. Set `TUILANE_CONFIG` to use another file, or pass a path to `config check`.

```toml
version = 1

[ui]
theme = "default" # default or high-contrast
layout = "balanced" # balanced or focus
keymap = "default" # default or vim
```

The `balanced` layout shows navigation when terminal width allows it; `focus` hides the navigation panel. The `high-contrast` theme uses brighter shell text and yellow accents. The `vim` keymap adds `j` and `k` view navigation when the active view does not handle those keys.

Validate the selected configuration without starting the TUI:

```sh
cargo run -p tuilane -- config check
cargo run -p tuilane -- config check ./tuilane.toml
```

The checker reports parse errors, unsupported versions, unknown fields, and unsupported preset values with their configuration path. Plugin-specific settings and interactive configuration editing are not implemented.
