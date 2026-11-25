# Minterm design system

The terminal owns the window. Leave the command line under the cursor when panels open, and return focus to the selected session when they close. Keep window management, menu shortcuts and Settings native to macOS.

## Type and color

[ui/tokens.css](ui/tokens.css) defines the application surfaces, text colors, type scale, spacing and control sizes. [ui/theme.js](ui/theme.js) reads the terminal palette and font metrics from those tokens once at startup. Shell prompts use ANSI colors so that the renderer controls their appearance.

| Role | Treatment |
| --- | --- |
| Command entry and output | Minterm Mono, with a shared fixed-width grid for regular and bold text |
| Tab labels, buttons and inspector text | Barlow Regular or Semibold |
| Paths, ports, process metadata and Git refs in the inspector | Martian Mono, using secondary text color for supporting detail |
| Active controls, branch and terminal cursor | Mint accent with a change in shape, surface or selection state |
| Tracked changes and errors | Git status in the tab tooltip, or an explicit error message in the affected control |

The fonts ship locally. Wait for both terminal weights before measuring the first grid. Minterm Mono uses squared bowls, diagonal openings, a marked zero and distinct shapes for `1`, `l` and `I`. Let the system provide glyphs outside its Latin and terminal-symbol repertoire. Keep ANSI colors distinct for tools that supply their own syntax highlighting.

## Components and layout

- Tabs share the native title bar with the new-tab and tool buttons. Icon buttons have an accessible name and a tooltip. The active tab has a filled surface and `aria-selected`. On hover or keyboard focus, the close button uses the same light surface and dark cross on active and inactive tabs.
- Projects opens a searchable dialog from the title bar or the native Shell menu. Rank folders by visits over seven days and show the path to distinguish identical names. Reuse an untouched shell for the selection; preserve input, drafts and agents by opening a new tab when the current tab has work. Escape restores terminal focus without changing the current session.
- The shell prompt on zsh and Bash uses the working directory, an optional Git ref and a chevron. Blue marks the directory, green marks the branch, and amber indicates a failed command. Keep prompt segments within the terminal cell grid.
- The terminal starts directly below the tabs. A tab tooltip carries the full path and tracked changes even while a full-screen CLI covers the prompt. An exited shell gets a recovery action next to its session.
- The inspector uses disclosure controls. On first launch, Agents starts expanded and Ports starts closed. Minterm remembers the inspector visibility, both disclosure states and the port filter. It combines local listeners and Docker port mappings in one scan and one list. On narrower windows the inspector overlays the terminal so full-screen CLI layouts retain their width.
- Switching modes keeps the shell and its agent inside the selected tab. At the shell prompt, a robot replaces the chevron while the user enters an agent task. Keep the directory, branch and terminal grid aligned when switching modes.
- Agent rows use the task as the primary label and the current action as supporting text. Reserve attention colors and text for confirmed process limits or a visible request for input.
- Use the spacing scale for ordinary control groups. Use alignment and surface changes to group information. Keep the native traffic-light clearance and the measured terminal cell dimensions explicit.
- In native Settings, use one command list and editor for the included agents and saved profiles. Mark the default in its row. Give the shell command field enough width to read a model name and its options together. Profile shortcuts belong to the robot prompt. Once a CLI has started, it owns its keyboard input.

## Checking a change

Open the app near its minimum width and at its normal width. Type immediately after ⌘T, switch tabs, unfold a section and return to the terminal with the keyboard. Check focus visibility, truncation of long paths and branch names, and alignment of colored prompts with a full-screen CLI. Run the native regression test described in [AGENTS.md](AGENTS.md).
