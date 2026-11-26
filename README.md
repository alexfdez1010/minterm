# Minterm

<img src="macos/Minterm.svg" alt="" width="72" height="72">

**A minimal macOS terminal for working with coding agents from the keyboard.**

Run Codex, Claude Code and OpenCode side by side in one window. Jump from an agent to its shell to check a diff or run a test, then pick up where you left off.

## Install and open Minterm

### Download for Apple Silicon

[**Download the latest Minterm DMG**](https://github.com/alexfdez1010/minterm/releases/latest/download/Minterm-arm64.dmg)

Requires an Apple Silicon Mac with macOS 13 or later. Sign in to GitHub with an account that has access to this private repository.

1. Open the downloaded `Minterm-arm64.dmg`.
2. Drag **Minterm** to **Applications**.
3. Open **Minterm** from **Applications**.

The app uses ad hoc signing. If macOS blocks the first launch, go to **System Settings → Privacy & Security** after attempting to open Minterm. Choose **Open Anyway** if it appears. [Apple’s instructions](https://support.apple.com/en-us/102445)

### Build from source

Install Rust 1.90 or later and the Xcode Command Line Tools (`xcode-select --install`). [Install Rust](https://rust-lang.org/tools/install/)

```sh
git clone https://github.com/alexfdez1010/minterm.git
cd minterm
./macos/bundle.sh --install && open "$HOME/Applications/Minterm.app"
```

The script builds, signs and installs the app in `~/Applications`. To open a particular project, run the installed executable from that project’s directory. Automatic updates are off by default; enable **Update on startup** under Settings → General → Updates. Before opening terminal sessions, Minterm checks for a new release, shows download progress, verifies its signature and installs it with an automatic restart in the requested directory. Choose **Open Minterm** to skip the update or continue after an error. You can also update manually by installing the latest DMG or rebuilding from source.

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
- **Find the session that needs you.** Press **⌘⇧A** to open the running agents list. The inspector recognizes Codex, Claude Code, OpenCode and Antigravity started from the shell too. Select a session marked **Input requested** to answer in its terminal.
- **Return to a project.** Click **Projects** or press **⌘⇧P** to search the folders you visit most over seven days and open a terminal there. [How projects are counted](docs/workspace.md#projects)
- **See what’s running locally.** The inspector shows listening TCP ports and optional Docker mappings. Tabs show the working directory and Git ref on hover, and zsh and Bash can show them in the prompt.

[Workspace guide](docs/workspace.md#the-terminal-workspace)

## ⌨️ Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| ⌘T | Open a tab in the active session’s directory |
| ⌘⇧P by default | Find a frequent project and open a terminal there |
| ⌘K | Switch between a shell and its agent |
| ⌘L | Toggle Split View |
| ⌘1 through ⌘9 | Select a tab or pane by position |
| ⌘← / ⌘→ | Focus the previous or next terminal, including in Split View |
| ⌃Tab / ⌃⇧Tab | Select the next or previous tab |
| ⌘⇧A | Open the running agents list |
| ⌘I | Show or hide the local inspector |
| ⌘⇧Return | Focus the terminal or restore its panels |
| ⌃L / ⌃U | Launch the light or ultra Codex profile from a shell or the robot prompt |
| ⌃A | Launch Antigravity from a shell or the robot prompt |
| ↑ / ↓ | Browse previous prompts at the robot prompt |
| ⌘W | Close the current tab or Settings; in Projects, remove the focused project from history. Press again to confirm closing a busy tab. |
| ⌘+ / ⌘− / ⌘0 | Increase, decrease or reset the terminal text size |
| ⌘, | Open Settings |

In **Settings → General**, change the **Switch mode**, **Toggle Split View** and **Open Project** shortcuts or the mode that **⌘T** opens. Record a Command shortcut with a letter and any additional modifiers you want. Changes apply immediately and are saved for the next launch.

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

Run `./macos/bundle.sh && uv run tests/macos/updater.py` to test startup updates with an isolated app, a local signed feed and a temporary signing key. It checks that an up-to-date app opens its terminal and that an available update installs and relaunches before any terminal starts.

### 🤝 Contributing

Open a GitHub issue for a bug or a feature idea. For terminal and shortcut bugs, include your macOS version, shell, reproduction steps and the agent CLI involved, if any. For a pull request, describe the behavior you changed and the checks you ran. [GitHub Issues](https://github.com/alexfdez1010/minterm/issues) · [Repository guide](AGENTS.md) · [Design system](DESIGN.md)

### Releases and bundled assets

After a push to `main`, GitHub Actions compares the app source and build files with the latest release. If they changed, it increments the patch version, tests the app and publishes the Apple Silicon DMG with a matching Git tag. Add `[release major]` to a commit message to increment the major version and reset the minor and patch numbers to zero. The workflow can also be run manually from `main` to publish pending app changes. [Release workflow](.github/workflows/release.yml)

The Rust updater verifies each DMG with the existing Ed25519 public key before mounting it. It validates the replacement bundle, exchanges it atomically with the installed app and keeps the previous copy until relaunch succeeds. Updates require write access to the app's folder; errors leave the installed app available. No Sparkle framework ships in the app.

The release workflow retains the signed appcast and DMG format so existing Sparkle installations can upgrade. It signs both with the `SPARKLE_PRIVATE_KEY` repository secret. The corresponding key is stored in the login keychain under the Sparkle account `minterm`; keep a secure backup, since losing it prevents existing installations from accepting future automatic updates. The repository and its releases must be public for automatic checks to work without credentials. Only release signing uses the pinned Sparkle tools, whose download is verified with SHA-256.

The embedded renderer is xterm.js 6.0.0 with its fit addon 0.11.0. Martian Mono and Barlow ship under the SIL Open Font License. To regenerate the two Minterm Mono weights, run `uv run ui/fonts/build_minterm_mono.py`. [Renderer licenses](ui/vendor) · [Font licenses](ui/fonts)
