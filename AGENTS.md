# Working on Minterm

Minterm is a macOS terminal for CLI agents. Keep terminal input responsive during output bursts and service scans. Write UI copy in English. Use the global design-craft skill for visual and interaction changes when available. Keep window chrome compact and give each label an operational purpose.

## Where changes belong

| Area | Entry point |
| --- | --- |
| CLI, launch directory and hidden key prompt | `src/main.rs`, `src/key_command.rs` |
| AppKit window, native menus and local asset boundary | `src/lib.rs`, `src/menu.rs`, `src/assets.rs` |
| Typed UI messages and background jobs | `src/app.rs` |
| Login shells, PTY I/O, backpressure and cleanup | `src/sessions.rs` |
| Agent recognition, Git, TCP listeners and Docker | `src/agents.rs`, `src/inspect.rs` |
| Provider requests, encrypted store and native Settings | `src/assistant.rs`, `src/credentials.rs`, `src/settings.rs`, `macos/settings.m` |
| Tabs, workspace and inspector | `ui/app.js`, `ui/inspector.js`, `ui/assistant.js`, `ui/app.css` |

The project skill [minterm-macos](.agents/skills/minterm-macos/SKILL.md) covers changes crossing a shell, the event loop and the macOS window. It includes the opt-in native keyboard test.

## Implementation conventions

- The UI and renderer ship as local assets embedded by Rust. Register new modules in `src/assets.rs`. Preserve the local origin check, content policy and navigation boundary.
- Keep shell output as bytes through the PTY. Acknowledge each output block after the renderer consumes it. Shells continue running in inactive tabs.
- Let the OS menu own macOS commands. Keep new-tab creation and selection on the input path. Run Git, Docker, process scans, provider calls and credential writes outside that path.
- Keep service discovery read-only and bounded. Introduce new agent recognition with executable or package-entrypoint fixtures. Only process metadata needed by the UI crosses the bridge.
- A provider receives the submitted question. Store credentials through `src/credentials.rs`; the webview never needs a saved key. Use a temporary home and dummy keys to test persistence, rotation and concurrent opens.
- Preserve the upstream JS and licenses in `ui/vendor/`. Make application changes in the adjacent source modules. Check the bundled app if a change affects paths, signing or macOS startup.

Build and test commands are in [README.md](README.md#development). Update the keyboard table when changing a shortcut.
