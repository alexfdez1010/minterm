mod agents;
mod app;
mod assets;
mod assistant;
mod credentials;
mod inspect;
pub mod key_command;
mod menu;
mod prompt;
mod sessions;
mod settings;

use app::{Action, App, Message};
use std::path::PathBuf;
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::macos::{ActivationPolicy, EventLoopExtMacOS, WindowBuilderExtMacOS},
    window::{Theme, WindowBuilder},
};
use wry::{NewWindowResponse, WebViewBuilder};

/// Start the macOS event loop. A ready hook lets the native smoke test drive
/// the same window and WebView without activating another app.
pub fn launch(
    start_dir: PathBuf,
    visible: bool,
    on_ready: impl FnOnce(&tao::window::Window, &wry::WebView),
) -> Result<(), Box<dyn std::error::Error>> {
    let vault_error = credentials::rotate_on_launch().err();
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
    let menu = menu::install()?;
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let _ = menu_proxy.send_event(Message::Menu(event.id.0));
    }));
    let ipc = proxy.clone();
    let view = WebViewBuilder::new()
        // PTYs need renderer acknowledgements while the window is minimized.
        .with_background_throttling(wry::BackgroundThrottlingPolicy::Disabled)
        .with_incognito(!visible)
        .with_url(assets::URL)
        .with_background_color((19, 22, 24, 255))
        .with_custom_protocol("minterm".into(), assets::serve)
        .with_navigation_handler(|url| url == assets::URL)
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if request.uri() == assets::URL
                && let Ok(action) = serde_json::from_str::<Action>(request.body())
            {
                let _ = ipc.send_event(Message::Action(action));
            }
        })
        .build(&window)?;
    on_ready(&window, &view);
    let mut app = App::new(view, proxy, start_dir, vault_error);
    event_loop.run(move |event, _, control| {
        let _keep_menu = &menu;
        *control = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => app.act(Action::Quit { force: false }),
            Event::UserEvent(Message::Action(Action::Drag)) => {
                let _ = window.drag_window();
            }
            Event::UserEvent(Message::Menu(name)) if name == "settings" => settings::show(),
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
