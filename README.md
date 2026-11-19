# Minterm

Minterm is a small macOS terminal for working with command-line agents. Each tab keeps a login shell and can run an interactive coding agent beside it, each on its own pseudo-terminal. The inspector shows agents across tabs, listening TCP ports and Docker port mappings. Select an agent to focus its terminal.

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
| ⇧⌘K | Switch between the shell and its agent (configurable in Settings) |
| ⌘W | Close the current tab or Settings window |
| ⌘1 through ⌘9 | Select a tab by position |
| ⌃Tab / ⌃⇧Tab | Select the next or previous tab |
| ⌘⇧A | Open the running agents list |
| ⌘I | Show or hide the local inspector |
| ⌘J | Show or hide the assistant |
| ⌘⇧Return | Focus the terminal or restore its panels |
| ⌘, | Open Settings |

In zsh and Bash, the prompt shows the current folder, the Git branch and a chevron. The chevron turns amber when a command fails. A detached checkout shows its short commit ID. Minterm reads Git’s HEAD file to build the prompt. To keep your existing prompt, launch Minterm with `MINTERM_PROMPT=0`. Other shells keep their own prompt.

In zsh 5.9 or later, the first suggestion appears in gray as you type. Right Arrow accepts it at the end of the line. Matches come from recent shell history and command names. Tab keeps your usual completion. In Bash, Shift-Tab cycles through Readline’s completions when that binding is available. Set `MINTERM_AUTOSUGGEST=0` to disable Minterm’s suggestions.

Hover over a tab to see its full path, current Git ref and any changes to tracked files. The inspector opens with Agents expanded. Expand **Ports** to see local listeners, published Docker ports and databases without a host port.

The **Development** filter shows common development ports, database containers and Docker mappings whose host or target uses a development port. **All ports** includes other published container ports and every TCP listener visible to your macOS account. The process name and PID come from the listener. A label such as Vite describes the port’s usual purpose. The same list uses the local Docker context and recognizes database image names including PostgreSQL, pgvector, MySQL, MongoDB and Redis.

The agents list recognizes Codex, Claude Code, Gemini CLI, OpenCode, Aider, GitHub Copilot, Qwen Code, Crush and Cursor Agent in Minterm sessions. Selecting a background job opens its owning tab. Shell job control remains available there.

## Agent sessions

Press **⇧⌘K** or click the mode button in a terminal tab. Type a task and press Return to start the selected CLI in the shell’s current directory. Shift-Return inserts a newline. Submit an empty field to open the CLI at its own prompt. Switch back to the shell with the same shortcut. Both sessions keep their processes, input and scrollback.

**Agents → +** opens a new tab with your task and an interactive CLI. You can answer prompts, grant permissions and follow up directly in Codex or Claude Code. The menu beside the new-tab button lets you open either mode. In **Settings → General**, choose what ⌘T opens, select the agent command or its executable, and record a shortcut for switching modes. Install the CLI you choose and complete its normal sign-in or local model setup.

The inspector marks recognizable requests for input and lets you jump to that agent. Minterm samples the visible terminal near the cursor, so unusual prompts can remain marked as running. When you start an agent from Minterm, its provider can summarize the initial task, recent activity labels and run state. Codex uses GPT-6 Luna with an OpenAI key. Claude Code uses Claude Haiku 5.5 with an Anthropic key. Codex with Ollama uses your configured Ollama server. Without a matching cloud key the inspector keeps showing local activity. Each session requests at most four summaries, at least 30 seconds apart. For an agent you start yourself in a shell, review and edit the excerpt using its summary button.

## Assistant and API keys

Open **Settings → API Keys** to save or remove a key for OpenAI or Anthropic. You can also enter it at a hidden prompt inside a Minterm shell.

```sh
minterm key set openai
minterm key set anthropic
```

Open the assistant with ⌘J, choose a provider and type a question. **Explain selection** or **Explain recent output** adds an editable excerpt. After receiving a reply, you can turn it into an editable task for the agent in the source tab. To use an environment key, set `OPEN_API_KEY` or `ANTHROPIC_API_KEY` in the process that launches Minterm. A saved key takes priority. The assistant’s question-answering model can be overridden with `MINTERM_AI_MODEL` for OpenAI or `MINTERM_ANTHROPIC_MODEL` for Anthropic.

Keys persist under `~/Library/Application Support/Minterm`. Minterm uses XChaCha20-Poly1305 and re-encrypts the store with a new random key each time the app launches. The store and master key have owner-only permissions. A process that can read both files can recover the API keys. To clear a stored key from the shell, use `minterm key remove openai` or `minterm key remove anthropic`.

### Ollama

Start your Ollama server and pull a model. In **Settings → Ollama**, enter the server address and choose an installed model from **Model**. **Refresh** reloads the list. You can also enter a model name directly. The server defaults to `http://127.0.0.1:11434`. For assistant questions, leaving Model empty uses the first model the server lists.

Choose **Ollama** in the assistant to send questions to that server. Choose **Codex with Ollama** under **Settings → General → Agent command** to use the same server and selected model in the interactive Codex CLI. Use a tool-capable model for coding tasks. [Ollama with Codex](https://docs.ollama.com/integrations/codex)

## Development

Run `cargo run --release --locked -- /path/to/project` to launch from source. To build and open a separate app in the checkout, run `./macos/bundle.sh && open dist/Minterm.app`.

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo test --locked --features ui-tests --test macos-ui
```

The last command drives AppKit and WebKit in a logged-in macOS session with an isolated home. The [assistant interaction proposal](docs/assistant.md) describes two possible follow-ups for failed commands and port conflicts. See [AGENTS.md](AGENTS.md) for the source layout and the repository skill for changing terminal behavior.

The embedded renderer is xterm.js 6.0.0 with its fit addon 0.11.0. Their upstream licenses are included in [ui/vendor](ui/vendor).

Martian Mono and Barlow are bundled under the SIL Open Font License. See [font licenses](ui/fonts) and the [design system](DESIGN.md).
