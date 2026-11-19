# Working on Minterm

Minterm is a macOS terminal for CLI agents. Keep terminal input responsive during output bursts and service scans. Write UI copy in English. Use the global design-craft skill for visual and interaction changes when available. Keep window chrome compact and give each label an operational purpose.

## Install and open

Use macOS 13 or later with Rust 1.90 or newer and the Xcode Command Line Tools. From the repository root, install for the current user and launch the app with one command.

```sh
./macos/bundle.sh --install && open "$HOME/Applications/Minterm.app"
```

For local development, `./macos/bundle.sh` builds the signed app in `dist/Minterm.app`. Open that copy with `open dist/Minterm.app`. [First-time setup](README.md#install-and-open-minterm)

## Where changes belong

| Area | Entry point |
| --- | --- |
| CLI and launch directory | `src/main.rs` |
| AppKit window, native menus and local asset boundary | `src/lib.rs`, `src/menu.rs`, `src/assets.rs` |
| Typed UI messages and session ownership | `src/app.rs` |
| PTY I/O, backpressure and cleanup | `src/sessions.rs` |
| Login shell integration and Git prompt | `src/prompt.rs`, `shell/` |
| Agent recognition, Git, TCP listeners and Docker | `src/agents.rs`, `src/inspect.rs` |
| Interactive agent executable resolution and launch | `src/agent_cli.rs`, `src/sessions.rs` |
| Local agent activity and launch dialog | `ui/activity.js`, `ui/agents.js` |
| Agent profiles, preferences and native Settings | `config/agents.toml`, `src/agent_config.rs`, `src/preferences.rs`, `macos/preferences.m`, `macos/agent_commands.m` |
| Tabs, agent mode and inspector | `ui/app.js`, `ui/command.js`, `ui/inspector.js`, `ui/app.css` |

Use [DESIGN.md](DESIGN.md) and the tokens in `ui/tokens.css` when extending the workspace.

The project skill [minterm-macos](.agents/skills/minterm-macos/SKILL.md) covers changes crossing a shell, the event loop and the macOS window. It includes the opt-in native keyboard test.

## Implementation conventions

- The UI and renderer ship as local assets embedded by Rust. Register new modules in `src/assets.rs`. Preserve the local origin check, content policy and navigation boundary.
- Keep shell output as bytes through the PTY. Acknowledge each output block after the renderer consumes it. Each tab owns a shell and an optional interactive agent with a separate PTY. Address input, resize and output acknowledgements by PTY ID, including while that surface is hidden. Resolve the shell’s live working directory when starting its agent.
- Let the OS menu own macOS commands. Keep new-tab creation and selection on the input path. Run Git, Docker and process scans outside that path.
- To configure an installed Minterm, edit the user’s `agents.toml` described in the [agent commands guide](README.md#agent-commands). The repository copy seeds new installations. Keep tasks and working directories in process arguments when extending command launch. Only the saved command itself is shell source.
- Keep service discovery read-only and bounded. Introduce new agent recognition with executable or package-entrypoint fixtures. Only process metadata needed by the UI crosses the bridge.
- Preserve the upstream JS and licenses in `ui/vendor/`. Make application changes in the adjacent source modules. Check the bundled app if a change affects paths, signing or macOS startup.

Build and test commands are in [README.md](README.md#development). Update the keyboard table when changing a shortcut.
