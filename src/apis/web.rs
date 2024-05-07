use http_body_util::Full;
use httpserver::{Bytes, HttpContext, HttpResponse, CONTENT_TYPE};
use hyper::StatusCode;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "resources/"]
#[exclude = "css/*"]
#[exclude = "img/*"]
#[exclude = "js/*"]
struct Asset;

pub async fn default_handler(ctx: HttpContext) -> HttpResponse {
    debug_assert!(!ctx.req.uri().path().is_empty());
    let ac = crate::AppConf::get();
    let mut path = &ctx.req.uri().path()[1..];
    if !ac.no_root && path.is_empty() {
        path = &"index.html";
    }

    let f = match Asset::get(path) {
        Some(f) => f,
        None => return resp(hyper::StatusCode::NOT_FOUND, "plain", "Not Found"),
    };

    let ext = match std::path::Path::new(&path).extension() {
        Some(s) => s.to_str().unwrap(),
        None => "",
    };

    resp(StatusCode::OK, ext, f.data.to_vec())
}

fn resp<T: Into<Bytes>>(status: StatusCode, content_type: &str, body: T) -> HttpResponse {
    Ok(
        hyper::Response::builder()
            .status(status)
            .header(CONTENT_TYPE, map_content_type(content_type))
            .body(Full::new(body.into()))?
    )
}

fn map_content_type(file_type: &str) -> &'static str {
    match file_type {
        "plain" => "text/plain",
        "html" => "text/html",
        "css"  => "text/css",
        "js"   => "application/javascript",
        "ico"  => "image/x-icon",
        "png"  => "image/png",
        "jpg"  => "image/jpeg",
        "gif"  => "image/gif",
        _      => "text/plain",
    }
}
