# Minterm

Minterm is a small macOS terminal for working with command-line agents. Each tab runs a login shell on a real pseudo-terminal. The inspector shows agents across tabs, listening TCP ports and local Docker databases. Select an agent to focus its terminal.

## Run on macOS

Use macOS 13 or later, Rust 1.90 or later and the Xcode Command Line Tools. From the checkout, build and open the application with the following commands.

```sh
./macos/bundle.sh
open dist/Minterm.app
```

You can move the resulting app to Applications. The script signs it locally for the Mac that built it. To start directly in a project from an existing shell, use `cargo run --release --locked -- /path/to/project` or run the built `target/release/minterm` executable there.

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

## Assistant and API keys

Open **Minterm → Settings…** to save or remove a key for OpenAI or Anthropic. You can also enter it at a hidden prompt inside a Minterm shell.

```sh
minterm key set openai
minterm key set anthropic
```

Choose the provider with ⌘J and type a question. Minterm sends the text you submit and displays the reply as plain text. It leaves command execution to you. To use an environment key, set `OPEN_API_KEY` or `ANTHROPIC_API_KEY` in the process that launches Minterm. A saved key takes priority. The model can be overridden with `MINTERM_AI_MODEL` for OpenAI or `MINTERM_ANTHROPIC_MODEL` for Anthropic.

Keys persist under `~/Library/Application Support/Minterm`. Minterm uses XChaCha20-Poly1305 and re-encrypts the store with a new random key each time the app launches. The store and master key have owner-only permissions. A process that can read both files can recover the API keys. To clear a stored key from the shell, use `minterm key remove openai` or `minterm key remove anthropic`.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo test --locked --features ui-tests --test macos-ui
```

The last command drives AppKit and WebKit in a logged-in macOS session with an isolated home. See [AGENTS.md](AGENTS.md) for the source layout and the repository skill for changing terminal behavior.

The embedded renderer is xterm.js 6.0.0 with its fit addon 0.11.0. Their upstream licenses are included in [ui/vendor](ui/vendor).

Martian Mono and Barlow are bundled under the SIL Open Font License. See [font licenses](ui/fonts) and the [design system](DESIGN.md).
