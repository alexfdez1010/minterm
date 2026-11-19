---
name: minterm-macos
description: Change or verify Minterm PTY sessions, keyboard shortcuts, agent navigation, or macOS window behavior across the native host and its terminal renderer.
---

# Minterm on macOS

Trace the interaction from the native menu or the focused WebKit control through `Action` in `src/app.rs`. PTY workers own shell I/O, the Tao event loop owns the window, and xterm consumes output asynchronously. Keep the acknowledgement tied to the completion of `term.write` when changing any part of that path.

## Testing the affected interaction

- For tab or shortcut work, run `cargo test --locked --features ui-tests --test macos-ui` from the repository root in a logged-in macOS session. The driver dispatches AppKit key events to a hidden window and verifies the PTY, the selected tab and the live Git branch. Browser-only keyboard events miss native menu handling. The harness uses its own home and repository.
- For shell lifecycle changes, run `cargo test --locked sessions::`. Exercise switching between a tab’s shell and agent, submitting a literal prompt after changing directories, exit, Ctrl-C, resizing and output beyond the acknowledgement window. Check a background job as well as the foreground group when changing close behavior.
- For agent or inspector changes, run `cargo test --locked agents::` or `cargo test --locked inspect::`. Use temporary PTYs or parser fixtures to exercise agent wrappers, atypical host ports and an unavailable Docker daemon.
- For a visual change, build with `./macos/bundle.sh` and inspect the resulting app at its usual width and near its minimum. Check keyboard focus after opening and closing the inspector or Settings. A tab opening while the inspector is refreshing should accept input immediately.
- When reporting a speed or size improvement, compare release builds on the same Mac. Distinguish time to create a tab from time to the first shell prompt, which includes the user’s login configuration.
