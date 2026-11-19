# Minterm

Minterm is a small macOS terminal for working with command-line agents. Each tab runs a login shell on a real pseudo-terminal. The inspector shows agents across tabs, listening TCP ports and local Docker databases. Select an agent to focus its terminal.

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
| ⌘W | Close the current tab or Settings window |
| ⌘1 through ⌘9 | Select a tab by position |
| ⌃Tab / ⌃⇧Tab | Select the next or previous tab |
| ⌘⇧A | Open the running agents list |
| ⌘I | Show or hide the local inspector |
| ⌘J | Show or hide the assistant |
| ⌘⇧Return | Focus the terminal or restore its panels |
| ⌘, | Open Settings |

In zsh and Bash, the prompt shows the current folder, the Git branch and a chevron. The chevron turns amber when a command fails. A detached checkout shows its short commit ID. Minterm reads Git’s HEAD file to build the prompt. To keep your existing prompt, launch Minterm with `MINTERM_PROMPT=0`. Other shells keep their own prompt.

Hover over a tab to see its full path, current Git ref and any changes to tracked files. The inspector opens with Agents expanded. Expand Listening ports or Docker databases to start a service scan. Each section collapses independently.

The **Development** filter prioritizes common web and database ports and recognized database mappings from Docker. **All ports** shows all TCP listeners visible to your macOS account. The process name and PID come from the listener. A label such as Vite describes the port’s usual purpose. The Docker section uses the local Docker context and recognizes database image names including PostgreSQL, pgvector, MySQL, MongoDB and Redis.

The agents list recognizes Codex, Claude Code, Gemini CLI, OpenCode, Aider, GitHub Copilot, Qwen Code, Crush and Cursor Agent in Minterm sessions. Selecting a background job opens its owning tab. Shell job control remains available there.

## Agent sessions

Open **Agents** with ⌘⇧A. The **+** beside the section starts an installed, signed-in Codex or Claude Code CLI in the current directory. Enter the task and press ⌘Return. The task names the new tab and travels to the CLI over standard input. These sessions use `codex exec --json -` or `claude -p --verbose --output-format stream-json` with the CLI’s configured permissions and account. Press Control-C in the run’s tab to stop it.

The inspector shows activity from the CLI’s structured events, including reading files, running commands and writing a response. The tab retains the final output. A permission or usage limit appears as **Needs attention** when the CLI reports a blocker. For interactive agents you started in a shell, **Input requested** identifies recognizable confirmation prompts near the cursor. Select the row to answer in that shell. An unfamiliar prompt may remain marked as running.

When you start a session from Minterm, Codex uses GPT-6 Luna for activity summaries if you have an OpenAI key. Claude Code uses Claude Haiku 5.5 if you have an Anthropic key. The provider receives the task, recent activity labels and run state. Without the matching key, the inspector shows local activity. Each run requests at most four summaries, at least 30 seconds apart. For agents started in a shell, the summary button lets you review and edit the excerpt before sending it. The CLIs use their own authentication.

## Assistant and API keys

Open **Minterm → Settings…** to save or remove a key for OpenAI or Anthropic. You can also enter it at a hidden prompt inside a Minterm shell.

```sh
minterm key set openai
minterm key set anthropic
```

Open the assistant with ⌘J, choose a provider and type a question. Minterm sends the text you submit and displays the reply as plain text. It leaves command execution to you. To use an environment key, set `OPEN_API_KEY` or `ANTHROPIC_API_KEY` in the process that launches Minterm. A saved key takes priority. The assistant’s question-answering model can be overridden with `MINTERM_AI_MODEL` for OpenAI or `MINTERM_ANTHROPIC_MODEL` for Anthropic.

Keys persist under `~/Library/Application Support/Minterm`. Minterm uses XChaCha20-Poly1305 and re-encrypts the store with a new random key each time the app launches. The store and master key have owner-only permissions. A process that can read both files can recover the API keys. To clear a stored key from the shell, use `minterm key remove openai` or `minterm key remove anthropic`.

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
