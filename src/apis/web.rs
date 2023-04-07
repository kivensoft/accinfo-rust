use httpserver::{HttpContext, Response};
use anyhow::Result;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "resources/"]
#[exclude = "css/*"]
#[exclude = "img/*"]
#[exclude = "js/*"]
struct Asset;

fn map_content_type(file_type: &str) -> &'static str {
    match file_type {
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

pub async fn default_handler(ctx: HttpContext) -> Result<Response> {
    assert!(ctx.req.uri().path().len() > 0);
    let ac = crate::AppConf::get();
    let mut path = &ctx.req.uri().path()[1..];
    if !ac.no_root && path.len() == 0 {
        path = &"index.html";
    }
    let path = path;

    let f = match Asset::get(&path) {
        Some(f) => f,
        None => {
            return hyper::Response::builder()
                .status(hyper::StatusCode::NOT_FOUND)
                .body(hyper::body::Body::empty())
                .map_err(|e| anyhow::anyhow!(e));
        },
    };

    let ext = match std::path::Path::new(&path).extension() {
        Some(s) => s.to_str().unwrap(),
        None => "",
    };

    hyper::Response::builder()
        .header("Content-Type", map_content_type(ext))
        .body(hyper::Body::from(f.data))
        .map_err(|e| anyhow::anyhow!(e))
}
