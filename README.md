# Minterm

<img src="macos/Minterm.svg" alt="" width="72" height="72">

**A minimal macOS terminal for working with coding agents from the keyboard.**

Run Codex, Claude Code and OpenCode side by side in one window. Jump from an agent to its shell to check a diff or run a test, then pick up where you left off.

## Install and open Minterm

Minterm runs on **macOS 13 or later**. The downloadable app is built for **Apple Silicon**.

### 📦 Download the app

Download `Minterm-arm64.zip`, unzip it and move `Minterm.app` to Applications. [Download from GitHub Releases](https://github.com/alexfdez1010/minterm/releases)

The app is ad hoc signed. If macOS asks you to approve the first launch, open **System Settings → Privacy & Security** and follow the macOS instructions. [Apple’s instructions](https://support.apple.com/en-us/102445)

The current ZIP, `v0.1.0`, predates Split View. Use the source build below to try the workspace described here.

### 🛠️ Build from source

Install Rust 1.90 or later and the Xcode Command Line Tools (`xcode-select --install`). [Install Rust](https://rust-lang.org/tools/install/)

```sh
git clone https://github.com/alexfdez1010/minterm.git
cd minterm
./macos/bundle.sh --install && open "$HOME/Applications/Minterm.app"
```

The script builds, signs and installs the app in `~/Applications`. To update, quit Minterm, pull the latest changes and rerun the last command. To open a particular project, run the installed executable from that project’s directory.

```sh
"$HOME/Applications/Minterm.app/Contents/MacOS/minterm" "$PWD"
```

## ⚡ Start an agent

Install and set up the CLI you want to use. To pick your default, open **Settings → Commands**, select its row and choose **Set as Default**.

1. Press **⌘K**, type a task at the robot prompt and press **Return**. Press Return with an empty prompt to start an interactive session.
2. Press **⌘K** again to switch between the agent and its shell. Each keeps its own processes and scrollback.
3. Open a second tab with **⌘T**. Press **⌘L** or click **Split View** in the title bar to see both tabs. Use **⌘1** through **⌘9** to move the keyboard focus between panes.

## 🪟 The workspace

- **Keep several sessions in view.** Split View tiles your open tabs. Add another agent, keep build output beside it or return to one tab while everything keeps running.
- **Find the session that needs you.** Press **⌘⇧A** to open the running agents list. The inspector recognizes Codex, Claude Code and OpenCode started from the shell too. Select a session marked **Input requested** to answer in its terminal.
- **See what’s running locally.** The inspector shows listening TCP ports and optional Docker mappings. Tabs show the working directory and Git ref on hover, and zsh and Bash can show them in the prompt.

[Workspace guide](docs/workspace.md#the-terminal-workspace)

## ⌨️ Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| ⌘T | Open a tab in the active session’s directory |
| ⌘K | Switch between a shell and its agent |
| ⌘L | Toggle Split View |
| ⌘1 through ⌘9 | Select a tab or pane by position |
| ⌃Tab / ⌃⇧Tab | Select the next or previous tab |
| ⌘⇧A | Open the running agents list |
| ⌘I | Show or hide the local inspector |
| ⌘⇧Return | Focus the terminal or restore its panels |
| ⌃L / ⌃U | Launch the light or ultra Codex profile from the robot prompt |
| ⌘W | Close the current tab or Settings. Press again to confirm closing a busy tab. |
| ⌘+ / ⌘− / ⌘0 | Increase, decrease or reset the terminal text size |
| ⌘, | Open Settings |

In **Settings → General**, change **Switch mode**, **Toggle Split View** or the mode that **⌘T** opens. Record a Command shortcut with a letter and any additional modifiers you want.

## Agent commands

In **Settings → Commands**, double-click a row to edit the shell invocation, model options or shortcut. You can also edit `~/.config/minterm/agents.toml` directly. To use a preset from the robot prompt, type its name followed by your task, for example `sol-light Review the current diff`. [Agent command guide](docs/workspace.md#agent-commands)

## Development

Minterm uses a Rust host, macOS WebKit and bundled xterm.js. It builds without Node.js and runs the frontend locally.

Run `cargo run --release --locked -- /path/to/project` to launch from source, or `./macos/bundle.sh && open dist/Minterm.app` to build and open a separate app in the checkout.

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo test --locked --features ui-tests --test macos-ui
```

The last command drives AppKit and WebKit in a logged-in macOS session with an isolated home.

### 🤝 Contributing

Open a GitHub issue for a bug or a feature idea. For terminal and shortcut bugs, include your macOS version, shell, reproduction steps and the agent CLI involved, if any. For a pull request, describe the behavior you changed and the checks you ran. [GitHub Issues](https://github.com/alexfdez1010/minterm/issues) · [Repository guide](AGENTS.md) · [Design system](DESIGN.md)

### Releases and bundled assets

To publish a release, update the version in `Cargo.toml`, `Cargo.lock` and `macos/Info.plist`, then push the matching version tag. GitHub Actions tests and packages the Apple Silicon build before publishing the ZIP. [Release workflow](.github/workflows/release.yml)

The embedded renderer is xterm.js 6.0.0 with its fit addon 0.11.0. Martian Mono and Barlow ship under the SIL Open Font License. To regenerate the two Minterm Mono weights, run `uv run ui/fonts/build_minterm_mono.py`. [Renderer licenses](ui/vendor) · [Font licenses](ui/fonts)
