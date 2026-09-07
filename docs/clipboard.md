# Clipboard over SSH

The default `native` backend uses the clipboard on the machine running sabiql. To send copy requests to your terminal instead, add this top-level setting to your existing `connections.toml`, before any `[[connections]]` section, then restart sabiql:

```toml
clipboard_backend = "osc52"
```

The configuration file is in `~/Library/Application Support/sabiql/` on macOS and `~/.config/sabiql/` on Linux (or the configured XDG configuration directory). Omit the setting or use `"native"` to retain the default. Other backend names are rejected. Saving connections or changing UI settings preserves this setting. There is no terminal detection, automatic fallback, or new native Wayland support.

OSC 52 sends the existing copy action's complete UTF-8 content, Base64-encoded, to terminal stdout. Cell, row, DDL, SQL/plan, connection-error, and JSON/detail actions retain their existing value selection and formatting, including error masking and row escaping; screen truncation is not applied by the backend. stdout must be a terminal. The output and the complete rendering frame share the standard stdout lock.

The notification **“Sent via OSC 52; clipboard acceptance unverified”** means the output was written and flushed. It does not mean the terminal accepted it or that pasting will work. Native copy-success feedback is not shown for this backend. Disabled or unsupported terminal clipboard operations can silently ignore the request. Local output failures and size rejections produce an error, with no native fallback. A partial output failure may already have sent some bytes.

sabiql limits each request to **74,994 UTF-8 bytes** before encoding. Base64 expands this to at most 99,992 bytes; `ESC ] 52 ; c ;` plus `BEL` adds eight bytes, for at most 100,000 output bytes. This is an application limit, not a universal terminal capacity. Larger values are rejected before any output, never truncated or split. Terminals may impose smaller limits.

The receiving terminal must support and permit clipboard writes via [OSC 52](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html). Over SSH, this means the local terminal and every intervening multiplexer. In tmux, check its [clipboard configuration](https://github.com/tmux/tmux/wiki/Clipboard), including `set-clipboard on` (the `external` setting blocks requests from applications inside tmux) and the outer terminal's clipboard capability. sabiql sends ordinary OSC 52; it does not add tmux passthrough wrappers or configure tmux for you.

Validation uses local simulated writers and a local PTY with the actual renderer. Real SSH, tmux, foot, Cloud Shell, and terminal clipboard acceptance have not been tested for this implementation; these are not certified configurations.
