# Minterm design system

The terminal owns the window. Leave the command line under the cursor when panels open, and return focus to the selected session when they close. Keep window management, menu shortcuts, secure entry and Settings native to macOS.

## Type and color

[ui/tokens.css](ui/tokens.css) defines the application surfaces, text colors, type scale, spacing and control sizes. [ui/theme.js](ui/theme.js) reads the terminal palette and font metrics from those tokens once at startup. Shell prompts use ANSI colors so that the renderer controls their appearance.

| Role | Treatment |
| --- | --- |
| Command entry, output and ports | Martian Mono, with one fixed-width cell grid per terminal |
| Tab labels, buttons and inspector text | Barlow Regular or Semibold |
| Paths, process metadata and Git refs | Monospace, using secondary text color for supporting detail |
| Active controls, branch and terminal cursor | Mint accent with a change in shape, surface or selection state |
| Tracked changes and errors | Git status in the tab tooltip, or an explicit error message in the affected control |

The fonts ship locally. Wait for the terminal font before measuring the first grid. Let the system provide glyphs outside the bundled Latin subset. Keep ANSI colors distinct for tools that supply their own syntax highlighting.

## Components and layout

- Tabs share the native title bar with the new-tab and tool buttons. Icon buttons have an accessible name and a tooltip. The active tab has a filled surface and `aria-selected`.
- The shell prompt on zsh and Bash uses the working directory, an optional Git ref and a chevron. Blue marks the directory, green marks the branch, and amber indicates a failed command. Keep prompt segments within the terminal cell grid.
- The terminal starts directly below the tabs. A tab tooltip carries the full path and tracked changes even while a full-screen CLI covers the prompt. An exited shell gets a recovery action next to its session.
- The inspector uses disclosure controls. Agents starts expanded. Ports and databases start closed and request data when opened. On narrower windows the inspector overlays the terminal so full-screen CLI layouts retain their width.
- Use the spacing scale for ordinary control groups. Use alignment and surface changes to group information. Keep the native traffic-light clearance and the measured terminal cell dimensions explicit.
- Preserve native Settings labels and controls. The assistant’s provider selection, question and reply belong beside the shell.

## Checking a change

Open the app near its minimum width and at its normal width. Type immediately after ⌘T, switch tabs, unfold a section and return to the terminal with the keyboard. Check focus visibility, truncation of long paths and branch names, and alignment of colored prompts with a full-screen CLI. Run the native regression test described in [AGENTS.md](AGENTS.md).
