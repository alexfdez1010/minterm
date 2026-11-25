# Minterm workspace guide

## The terminal workspace

Use **Split View** in the title bar or **View → Toggle Split View** to see your open terminals together. The default shortcut is **⌘L**. Two tabs sit side by side, three put the first terminal across the top, and four make a two-by-two grid. With more tabs, Minterm adds rows and scrolls to the selected pane. Click a pane or press **⌘1** through **⌘9** to move keyboard focus. The active pane has a mint outline. Switching views keeps your shells and agents running.

In **Settings → General**, click the shortcut beside **Toggle Split View** and press a Command key combination with a letter. Escape cancels recording. If you already assigned **⌘L** to **Switch mode** before this update, your binding stays in place and Split View starts with **⇧⌘L**.

### Shell and Git

In zsh and Bash, the prompt shows the current folder, the Git branch and a chevron that turns amber when a command fails. A detached checkout shows its short commit ID. To keep your existing prompt, launch Minterm with `MINTERM_PROMPT=0`. Other shells keep their own prompt.

The active tab shows a preview of the latest shell command. Bash uses commands saved to history and updates the tab when its prompt returns. Hover over a tab to see its full path, current Git ref and changes to tracked files.

In zsh 5.9 or later, the first suggestion appears in gray as you type. Press Right Arrow at the end of the line to accept it. Matches come from recent history and command names. Tab runs your usual completion. In Bash, Shift-Tab cycles through Readline’s completions when that binding is available. Set `MINTERM_AUTOSUGGEST=0` to disable Minterm’s suggestions.

Minterm disables macOS inline text predictions in terminal and task fields so suggestions cannot repeat characters already sent to a shell or agent.

### Projects

Click **Projects** or press **⌘⇧P** to open a new terminal in a frequently visited folder. Search by name or path, then press Return or select a row. Projects are ranked by visits during the last seven days, with recent visits breaking ties. Subfolders of a Git repository count toward its root, including Git worktrees; other folders count individually. Home and filesystem root are excluded.

Use a row’s remove button, or focus the project with the arrow keys and press **⌘W**, to remove it from history. Its folder and open terminals stay intact. A new visit adds the project again. **Escape** closes Projects; **⌘W** also closes it when the search field is focused.

Minterm learns from its own shell sessions, including background tabs, starting with this version. A visit is opening a shell in a folder or entering a different project; repeated checks and movement within the same repository do not add visits. Directory changes are sampled every two seconds and when terminal context refreshes, so very brief visits can be missed. History stays on this Mac in `~/Library/Application Support/minterm/projects.json` (or `$XDG_STATE_HOME/minterm/projects.json`), expires after seven days and retains at most 4,096 visits.

### Local inspector

Minterm opens the inspector with **Agents** expanded on first launch. Select a session to focus its terminal, or use **Agents → +** to open an agent in a new tab. Leave **Task** empty for an interactive session or enter a task to send at launch. Minterm detects agent activity locally and marks recognizable confirmation prompts as **Input requested**.

Expand **Ports** to see local TCP listeners, published Docker ports and database containers without a host port. The **Development** filter shows common development ports, database containers and Docker mappings whose host or target uses a development port. **All ports** also includes other published container ports and every TCP listener visible to your macOS account.

The process name and PID come from the listener. A label such as Vite describes the port’s usual purpose. Docker discovery uses the local context and recognizes database images including PostgreSQL, pgvector, MySQL, MongoDB and Redis.

## Agent commands

Install the agent CLI and complete its sign-in or local model setup. Then choose a default in **Settings → Commands**. In **Settings → General**, change the **Switch mode** shortcut or choose which mode opens with **⌘T**. On upgrade from the original mode shortcut, the previous default **⇧⌘K** becomes **⌘K**.

Press **⌘K** to open the robot prompt from a shell. Enter a task for the default agent, or start with a saved command name, such as `sol-light Review the current diff`. Shift-Return inserts a newline. Press Return at an empty prompt to launch without an initial task. The agent starts in the shell’s current directory. Press **⌘K** again to move between the shell and the running agent.

### Command presets

| Starter command | Launches | Shortcut at the robot prompt |
| --- | --- | --- |
| `codex`, `claude`, `opencode` | The corresponding interactive CLI | |
| `antigravity` | Antigravity CLI (`agy`) | ⌃A |
| `ocq` | Your shell’s `ocq` alias for OpenCode with Qwen through Ollama | |
| `sol-light` | Codex with `gpt-6.2-sol` and low reasoning effort | ⌃L |
| `sol-ultra` | Codex with `gpt-6.2-sol` and ultra reasoning effort | ⌃U |

Double-click a command in **Settings → Commands** to edit it. Click the shortcut recorder, or Tab to it from **Shell command** and press Space. Press Control and a letter together and choose **Save**. To remove a binding, click × or press Delete with the recorder focused, then save. Escape cancels recording.

With the robot prompt focused, press a profile’s shortcut to launch it with your draft. Existing bindings such as `Enter+L` also work with Control-L. You can still hold Return and press L for that binding. Running agent CLIs use their own keyboard controls.

The `ocq` preset calls `ocq --` so that `ollama launch` forwards the task to OpenCode. To change the model, edit your alias or set the profile’s shell command to `ollama launch opencode --model MODEL --`, replacing `MODEL` with its installed name. If your alias starts OpenCode directly, use `ocq` as the shell command.

### Editing the TOML file

Minterm copies its starter commands to `~/.config/minterm/agents.toml` on first launch. If `XDG_CONFIG_HOME` is an absolute path, it uses `$XDG_CONFIG_HOME/minterm/agents.toml`. **Show in Finder** opens the file’s location. [Starter TOML](../config/agents.toml)

Add a `[[commands]]` table for each profile.

| Field | Use |
| --- | --- |
| `name` | The name to type at the robot prompt |
| `agent` | The initial-task syntax to use, one of `codex`, `claude`, `opencode` or `antigravity` |
| `command` | The command to invoke in the interactive login shell |
| `shortcut` | An optional binding such as `"Ctrl+L"`. Use a distinct letter for each profile. |

Put persistent aliases or functions in the login shell’s startup files. To select the default profile, put a line such as `default = "sol-light"` above the first table. To use a changed profile, start a new agent session.

For Antigravity CLI, choose **Antigravity** as the base agent and use `agy` as the shell command. Minterm passes an initial task with `--prompt-interactive` so the session stays open for follow-up input. `agy --prompt` runs a single task and exits.

New configurations include Codex, Claude Code, OpenCode and Antigravity. The current file format is `version = 2`. Minterm upgrades earlier files on first use and keeps custom commands, executable paths and the selected default.

[Back to Minterm](../README.md)
