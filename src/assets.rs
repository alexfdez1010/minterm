use std::borrow::Cow;
use wry::http::{Request, Response};

pub const URL: &str = "minterm://localhost/";
pub const UPDATE_URL: &str = "minterm://localhost/update.html";

pub fn serve(_: wry::WebViewId<'_>, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let (mime, body): (&str, &[u8]) = match request.uri().path() {
        "/" => ("text/html", include_bytes!("../ui/index.html")),
        "/update.html" => ("text/html", include_bytes!("../ui/update.html")),
        "/update.css" => ("text/css", include_bytes!("../ui/update.css")),
        "/update.js" => ("text/javascript", include_bytes!("../ui/update.js")),
        "/tokens.css" => ("text/css", include_bytes!("../ui/tokens.css")),
        "/theme.js" => ("text/javascript", include_bytes!("../ui/theme.js")),
        "/fonts/barlow-regular.woff2" => (
            "font/woff2",
            include_bytes!("../ui/fonts/barlow-regular.woff2"),
        ),
        "/fonts/barlow-semibold.woff2" => (
            "font/woff2",
            include_bytes!("../ui/fonts/barlow-semibold.woff2"),
        ),
        "/fonts/martian-mono.woff2" => (
            "font/woff2",
            include_bytes!("../ui/fonts/martian-mono.woff2"),
        ),
        "/fonts/Minterm-Mono-Regular.woff2" => (
            "font/woff2",
            include_bytes!("../ui/fonts/Minterm-Mono-Regular.woff2"),
        ),
        "/fonts/Minterm-Mono-Bold.woff2" => (
            "font/woff2",
            include_bytes!("../ui/fonts/Minterm-Mono-Bold.woff2"),
        ),
        "/command.js" => ("text/javascript", include_bytes!("../ui/command.js")),
        "/activity.js" => ("text/javascript", include_bytes!("../ui/activity.js")),
        "/agents.js" => ("text/javascript", include_bytes!("../ui/agents.js")),
        "/agents.css" => ("text/css", include_bytes!("../ui/agents.css")),
        "/app.css" => ("text/css", include_bytes!("../ui/app.css")),
        "/inspector.js" => ("text/javascript", include_bytes!("../ui/inspector.js")),
        "/shared.js" => ("text/javascript", include_bytes!("../ui/shared.js")),
        "/terminal-input.js" => ("text/javascript", include_bytes!("../ui/terminal-input.js")),
        "/app.js" => ("text/javascript", include_bytes!("../ui/app.js")),
        "/projects.js" => ("text/javascript", include_bytes!("../ui/projects.js")),
        "/vendor/xterm.js" => ("text/javascript", include_bytes!("../ui/vendor/xterm.js")),
        "/vendor/addon-fit.js" => (
            "text/javascript",
            include_bytes!("../ui/vendor/addon-fit.js"),
        ),
        "/vendor/xterm.css" => ("text/css", include_bytes!("../ui/vendor/xterm.css")),
        _ => {
            return Response::builder()
                .status(404)
                .body(Cow::Borrowed(&[][..]))
                .unwrap();
        }
    };
    Response::builder()
        .header("Content-Type", mime)
        .header("X-Content-Type-Options", "nosniff")
        .header("Content-Security-Policy", "default-src 'none'; script-src 'self'; font-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-src 'none'; object-src 'none'")
        .body(Cow::Borrowed(body)).unwrap()
}
