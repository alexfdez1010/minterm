use std::borrow::Cow;
use wry::http::{Request, Response};

pub const URL: &str = "minterm://localhost/";

pub fn serve(_: wry::WebViewId<'_>, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let (mime, body): (&str, &[u8]) = match request.uri().path() {
        "/" => ("text/html", include_bytes!("../ui/index.html")),
        "/app.css" => ("text/css", include_bytes!("../ui/app.css")),
        "/assistant.js" => ("text/javascript", include_bytes!("../ui/assistant.js")),
        "/inspector.js" => ("text/javascript", include_bytes!("../ui/inspector.js")),
        "/shared.js" => ("text/javascript", include_bytes!("../ui/shared.js")),
        "/app.js" => ("text/javascript", include_bytes!("../ui/app.js")),
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
        .header("Content-Type", format!("{mime}; charset=utf-8"))
        .header("X-Content-Type-Options", "nosniff")
        .header("Content-Security-Policy", "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-src 'none'; object-src 'none'")
        .body(Cow::Borrowed(body)).unwrap()
}
