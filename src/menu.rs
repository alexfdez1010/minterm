use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{Menu, MenuItem, PredefinedMenuItem as Native, Submenu};

pub fn install() -> muda::Result<(Menu, MenuItem)> {
    let cmd = Modifiers::META;
    let item = |id, name, modifiers, key| {
        MenuItem::with_id(id, name, true, Some(Accelerator::new(modifiers, key)))
    };
    let app = Submenu::with_items(
        "Minterm",
        true,
        &[
            &Native::about(Some("About Minterm"), None),
            &item("settings", "Settings…", cmd, Code::Comma),
            &Native::hide(Some("Hide Minterm")),
            &item("quit", "Quit Minterm", cmd, Code::KeyQ),
        ],
    )?;
    let switch_mode = MenuItem::with_id("commandMode", "Switch Terminal / Agent Mode", true, None);
    switch_mode.set_key_accelerator(crate::preferences::accelerator(
        &crate::preferences::read().shortcut,
    ))?;
    let shell = Submenu::with_items(
        "Shell",
        true,
        &[
            &item("new", "New Tab", cmd, Code::KeyT),
            &MenuItem::with_id("newAgentTab", "New Agent Tab", true, None),
            &MenuItem::with_id("newTerminalTab", "New Terminal Tab", true, None),
            &switch_mode,
            &item("close", "Close Tab", cmd, Code::KeyW),
            &item(
                "previous",
                "Previous Tab",
                cmd | Modifiers::SHIFT,
                Code::BracketLeft,
            ),
            &item(
                "next",
                "Next Tab",
                cmd | Modifiers::SHIFT,
                Code::BracketRight,
            ),
        ],
    )?;
    let edit = Submenu::with_items(
        "Edit",
        true,
        &[
            &Native::undo(None),
            &Native::redo(None),
            &Native::cut(None),
            &Native::copy(None),
            &Native::paste(None),
            &Native::select_all(None),
        ],
    )?;
    let view = Submenu::with_items(
        "View",
        true,
        &[
            &item(
                "agents",
                "Running Agents",
                cmd | Modifiers::SHIFT,
                Code::KeyA,
            ),
            &item(
                "focus",
                "Focus Terminal",
                cmd | Modifiers::SHIFT,
                Code::Enter,
            ),
            &item("inspector", "Local Inspector", cmd, Code::KeyI),
            &item("larger", "Increase Terminal Text", cmd, Code::Equal),
            &item("smaller", "Decrease Terminal Text", cmd, Code::Minus),
            &item("reset-size", "Actual Text Size", cmd, Code::Digit0),
            &Native::fullscreen(None),
        ],
    )?;
    let window = Submenu::with_items(
        "Window",
        true,
        &[&Native::minimize(None), &Native::zoom(None)],
    )?;
    let menu = Menu::with_items(&[&app, &shell, &edit, &view, &window])?;
    menu.init_for_nsapp();
    Ok((menu, switch_mode))
}
