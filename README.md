# Minterm

Minterm is a small macOS terminal for working with command-line agents. Each tab keeps a login shell and can switch to an interactive coding agent, each on its own pseudo-terminal. The inspector shows agents across tabs, listening TCP ports and Docker port mappings. Select an agent to focus its terminal.

## Install and open Minterm

On macOS 13 or later, install Rust 1.90 or newer and the Xcode Command Line Tools. You can get the command line tools by running `xcode-select --install`. [Install Rust](https://rust-lang.org/tools/install/)

```sh
git clone https://github.com/alexfdez1010/minterm.git
cd minterm
./macos/bundle.sh --install && open "$HOME/Applications/Minterm.app"
```

The installer builds Minterm, signs it locally and installs it in your personal Applications folder. You can then open Minterm from Spotlight or Finder. To update, quit Minterm and run the last command again from an updated checkout.

To start Minterm in a project, run this command from that project's directory.

```sh
"$HOME/Applications/Minterm.app/Contents/MacOS/minterm" "$PWD"
```

The Rust host uses macOS WebKit and a bundled copy of xterm.js. Building and running the terminal requires neither Node.js nor an online frontend. Docker is optional.

## Working in the terminal

| Shortcut | Action |
| --- | --- |
| ⌘T | Open a tab in the active session’s directory |
| ⌘K | Switch between the shell and its agent (configurable in Settings) |
| ⌃L / ⌃U | Launch the light or ultra Codex profile from the robot prompt (configurable in Commands) |
| ⌘W | Close the current tab or Settings window |
| ⌘1 through ⌘9 | Select a tab by position |
| ⌃Tab / ⌃⇧Tab | Select the next or previous tab |
| ⌘⇧A | Open the running agents list |
| ⌘I | Show or hide the local inspector |
| ⌘⇧Return | Focus the terminal or restore its panels |
| ⌘+ | Increase terminal text size |
| ⌘− | Decrease terminal text size |
| ⌘0 | Restore the default terminal text size |
| ⌘, | Open Settings |

In zsh and Bash, the prompt shows the current folder, the Git branch and a chevron. The chevron turns amber when a command fails. A detached checkout shows its short commit ID. Minterm reads Git’s HEAD file to build the prompt. To keep your existing prompt, launch Minterm with `MINTERM_PROMPT=0`. Other shells keep their own prompt.

In zsh 5.9 or later, the first suggestion appears in gray as you type. Right Arrow accepts it at the end of the line. Matches come from recent shell history and command names. Tab keeps your usual completion. In Bash, Shift-Tab cycles through Readline’s completions when that binding is available. Set `MINTERM_AUTOSUGGEST=0` to disable Minterm’s suggestions.

Hover over a tab to see its full path, current Git ref and any changes to tracked files. The inspector opens with Agents expanded. Expand **Ports** to see local listeners, published Docker ports and databases without a host port.

The **Development** filter shows common development ports, database containers and Docker mappings whose host or target uses a development port. **All ports** includes other published container ports and every TCP listener visible to your macOS account. The process name and PID come from the listener. A label such as Vite describes the port’s usual purpose. The same list uses the local Docker context and recognizes database image names including PostgreSQL, pgvector, MySQL, MongoDB and Redis.

The agents list recognizes Codex, Claude Code and OpenCode, including sessions started directly from the shell. Selecting a background job opens its owning tab.

## Agent sessions

In **Settings → Commands**, select a command and choose **Set as Default**. Codex, Claude Code and OpenCode appear in the same list as your own commands. Double-click any row to edit its invocation, arguments or shortcut. Install the underlying CLI and complete its sign-in or local model setup. **General** controls the mode shortcut and which mode ⌘T opens.

On upgrade, the previous default ⇧⌘K changes to ⌘K. Other saved shortcuts are preserved.

Press **⌘K** in a shell. A robot replaces the prompt chevron. Type your task there and press Return to start the selected CLI in the shell’s current directory. Shift-Return inserts a newline. Press Return at an empty prompt to open the CLI without an initial task. Press ⌘K again to move between the shell and the running agent. Each keeps its own input, processes and scrollback.

**Agents → +** opens the selected agent in a new tab. Leave **Task** empty to start an interactive session, or enter a task to send when it starts. The inspector reads activity locally and marks recognizable confirmation prompts as **Input requested**. Select the agent to answer in its terminal.

### Agent commands

To assign or change a shortcut, open a command in **Settings → Commands**. Click the button beside **Shortcut**, press Control and a letter together, then choose **Save**. From the **Shell command** field, Tab to the recorder and press Space to start. Escape cancels recording. To remove a binding, click × or press Delete with the recorder focused, then save.

At the robot prompt, type a saved name and press Return to start it. To pass an initial task, type the name followed by the task, such as `ocq Review the current diff`. Other text goes to your default agent. A running agent keeps its own keyboard controls.

| Starter command | Launches | Shortcut at the robot prompt |
| --- | --- | --- |
| `codex`, `claude`, `opencode` | The corresponding interactive CLI | |
| `ocq` | Your shell’s `ocq` alias for OpenCode with Qwen through Ollama | |
| `sol-light` | Codex with `gpt-6.2-sol` and low reasoning effort | Control-L |
| `sol-ultra` | Codex with `gpt-6.2-sol` and ultra reasoning effort | Control-U |

With the robot prompt focused, press a profile’s shortcut to launch it with your draft. Leave the prompt empty to start a session without a task. Existing bindings such as `Enter+L` also work with Control-L. You can still hold Return and press L for that binding.

The `ocq` preset calls `ocq --` so that `ollama launch` forwards the task to OpenCode. To change the Ollama model, edit your alias or set the profile’s shell command to `ollama launch opencode --model MODEL --`, replacing `MODEL` with its installed name. If your alias starts OpenCode directly, use `ocq` as the shell command.

Edit profiles in **Settings → Commands** or in `~/.config/minterm/agents.toml`. When `XDG_CONFIG_HOME` is set to an absolute directory, Minterm uses `$XDG_CONFIG_HOME/minterm/agents.toml`. **Show in Finder** opens the file’s location. The first launch copies the [starter TOML](config/agents.toml) there.

In the current file format (`version = 2`), this list includes the three standard agents. Minterm upgrades earlier files on first use and keeps custom commands, executable paths and the selected default.

Add a `[[commands]]` table per profile. Its `name` is what you type, `agent` selects the initial-task syntax (`codex`, `claude` or `opencode`), and `command` is the invocation in your interactive login shell. Put persistent aliases or functions in that shell’s startup files. The optional `shortcut` takes the form `"Ctrl+L"`. Give each shortcut a different letter. To set the default in the file, put `default = "sol-light"` above the first table. To use a changed profile, return to Minterm and start a new agent session.

## Development

Run `cargo run --release --locked -- /path/to/project` to launch from source. To build and open a separate app in the checkout, run `./macos/bundle.sh && open dist/Minterm.app`.

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo test --locked --features ui-tests --test macos-ui
```

The last command drives AppKit and WebKit in a logged-in macOS session with an isolated home. See [AGENTS.md](AGENTS.md) for the source layout and the repository skill for changing terminal behavior.

The embedded renderer is xterm.js 6.0.0 with its fit addon 0.11.0. Their upstream licenses are included in [ui/vendor](ui/vendor).

Martian Mono and Barlow are bundled under the SIL Open Font License. See [font licenses](ui/fonts) and the [design system](DESIGN.md).
