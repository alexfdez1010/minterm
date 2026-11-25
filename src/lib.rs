mod agent_cli;
mod agent_config;
mod agents;
mod app;
mod assets;
mod inspect;
mod menu;
mod preferences;
mod projects;
mod prompt;
mod sessions;
mod settings;
mod updater;

use app::{Action, App, Message};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::macos::{ActivationPolicy, EventLoopExtMacOS, WindowBuilderExtMacOS},
    window::{Theme, WindowBuilder},
};
use wry::{NewWindowResponse, WebViewBuilder};

pub fn apply_update_if_requested() -> Result<bool, String> {
    updater::apply_if_requested()
}

/// Start the macOS event loop. A ready hook lets the native smoke test drive
/// the same window and WebView without activating another app.
pub fn launch(
    start_dir: PathBuf,
    visible: bool,
    on_ready: impl FnOnce(&tao::window::Window, &wry::WebView),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = EventLoopBuilder::<Message>::with_user_event().build();
    if !visible {
        event_loop.set_activation_policy(ActivationPolicy::Prohibited);
        event_loop.set_activate_ignoring_other_apps(false);
    }
    let window = WindowBuilder::new()
        .with_visible(visible)
        .with_title("Minterm")
        .with_theme(Some(Theme::Dark))
        .with_inner_size(LogicalSize::new(1120., 710.))
        .with_min_inner_size(LogicalSize::new(610., 390.))
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_fullsize_content_view(true)
        .with_traffic_light_inset(LogicalPosition::new(18., 18.))
        .build(&event_loop)?;
    let (menu, switch_mode, panes, projects) = menu::install()?;
    let bundle = updater::installed_bundle();
    let mut updating = visible
        && bundle.is_some()
        && preferences::read_general().automatic_updates
        && !std::env::args_os().any(|arg| arg == "--skip-update");
    let cancel_update = Arc::new(AtomicBool::new(false));
    let mut checking = false;
    let proxy = event_loop.create_proxy();
    settings::notify_changes(proxy.clone());
    let menu_proxy = proxy.clone();
    muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let _ = menu_proxy.send_event(Message::Menu(event.id.0));
    }));
    let ipc = proxy.clone();
    let view = WebViewBuilder::new()
        // A click can select a tab when returning from another app.
        .with_accept_first_mouse(true)
        // PTYs need renderer acknowledgements while the window is minimized.
        .with_background_throttling(wry::BackgroundThrottlingPolicy::Disabled)
        .with_incognito(!visible)
        .with_url(if updating {
            assets::UPDATE_URL
        } else {
            assets::URL
        })
        .with_background_color((19, 22, 24, 255))
        .with_custom_protocol("minterm".into(), assets::serve)
        .with_navigation_handler(|url| url == assets::URL || url == assets::UPDATE_URL)
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if request.uri() == assets::UPDATE_URL {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(request.body()) {
                    let message = match value["action"].as_str() {
                        Some("startUpdate") => Message::StartUpdate,
                        Some("cancelUpdate") => Message::CancelUpdate,
                        Some("drag") => Message::Action(Action::Drag),
                        _ => return,
                    };
                    let _ = ipc.send_event(message);
                }
                return;
            }
            if request.uri() == assets::URL
                && let Ok(action) = serde_json::from_str::<Action>(request.body())
            {
                let _ = ipc.send_event(Message::Action(action));
            }
        })
        .build(&window)?;
    on_ready(&window, &view);
    let update_proxy = proxy.clone();
    let update_directory = start_dir.clone();
    let mut app = App::new(view, proxy, start_dir);
    let main_window = window.id();
    event_loop.run(move |event, _, control| {
        let _keep_menu = &menu;
        *control = ControlFlow::Wait;
        if updating {
            match event {
                Event::UserEvent(Message::StartUpdate) if !checking => {
                    checking = true;
                    let proxy = update_proxy.clone();
                    let cancel = cancel_update.clone();
                    let bundle = bundle.clone().expect("updating an installed bundle");
                    std::thread::spawn(move || {
                        let result = updater::prepare(bundle, cancel, |progress| {
                            let _ = proxy.send_event(Message::UpdateProgress(progress));
                        })
                        .map_err(|error| error.to_string());
                        let _ = proxy.send_event(Message::UpdateComplete(result));
                    });
                }
                Event::UserEvent(Message::UpdateProgress(progress)) => {
                    app.update_progress(progress)
                }
                Event::UserEvent(Message::UpdateComplete(Ok(Some(prepared)))) => {
                    app.update_progress(updater::Progress {
                        status: "Installing update",
                        detail: String::new(),
                        fraction: None,
                    });
                    match prepared.install(update_directory.clone()) {
                        Ok(()) => *control = ControlFlow::Exit,
                        Err(error) => app.update_failed(&error.to_string()),
                    }
                }
                Event::UserEvent(Message::CancelUpdate | Message::UpdateComplete(Ok(None))) => {
                    cancel_update.store(true, Ordering::Relaxed);
                    updating = false;
                    app.open_workspace();
                }
                Event::UserEvent(Message::UpdateComplete(Err(error))) => app.update_failed(&error),
                Event::UserEvent(Message::Action(Action::Drag)) => {
                    let _ = window.drag_window();
                }
                Event::UserEvent(Message::Menu(name)) if name == "quit" => {
                    cancel_update.store(true, Ordering::Relaxed);
                    *control = ControlFlow::Exit;
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested | WindowEvent::Destroyed,
                    ..
                } => {
                    cancel_update.store(true, Ordering::Relaxed);
                    *control = ControlFlow::Exit;
                }
                _ => {}
            }
            return;
        }
        match event {
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } if window_id == main_window => app.act(Action::Quit { force: false }),
            // AppKit can destroy a window directly, without asking its delegate
            // for permission. There is no window left to host a confirmation.
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Destroyed,
                ..
            } if window_id == main_window => app.act(Action::Quit { force: true }),
            Event::UserEvent(Message::Action(Action::Drag)) => {
                let _ = window.drag_window();
            }
            Event::UserEvent(Message::Menu(name)) if name == "settings" => settings::show(&window),
            Event::UserEvent(Message::Preferences) => {
                let prefs = preferences::read_general();
                let _ = switch_mode.set_key_accelerator(preferences::accelerator(&prefs.shortcut));
                let _ = panes.set_key_accelerator(preferences::accelerator(&prefs.split_shortcut));
                let _ = projects
                    .set_key_accelerator(preferences::accelerator(&prefs.projects_shortcut));
                app.receive(Message::Preferences);
            }
            Event::UserEvent(Message::Menu(name))
                if name == "close" && settings::close_if_key() => {}
            Event::UserEvent(Message::Menu(name)) => {
                if visible {
                    window.set_focus();
                }
                app.receive(Message::Menu(name));
            }
            Event::UserEvent(message) => app.receive(message),
            _ => {}
        }
        if app.quitting() {
            *control = ControlFlow::Exit;
        }
    })
}
