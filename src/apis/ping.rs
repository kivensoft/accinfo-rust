use crate::httpserver::{HttpContext, ResBuiler, Response};
use anyhow::Result;
use serde::{Serialize, Deserialize};

pub async fn ping(ctx: HttpContext) -> Result<Response> {
    #[derive(Deserialize)] struct PingRequest { reply: Option<String> }

    #[derive(Serialize)]
    struct PingResponse {
        reply: String,
        now: String,
        server: String,
    }

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let server = format!("{}/{}", crate::APP_NAME, crate::APP_VER);
    let reply = match ctx.into_option_json::<PingRequest>().await? {
        Some(ping_params) => ping_params.reply,
        None => None,
    }.unwrap_or("pong".to_owned());

    ResBuiler::ok(&PingResponse { reply, now, server })
}
