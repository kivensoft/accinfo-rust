use std::{net::SocketAddr, sync::Mutex};

use crate::httpserver::{HttpContext, Response};
use anyhow::Result;
use hyper::client::{Client, HttpConnector};

lazy_static::lazy_static! {
    static ref PROXY_CLIENT: Client<HttpConnector> = Client::builder().build_http();
    static ref PROXY_ADDR: Mutex<SocketAddr> = Mutex::new(SocketAddr::from(([127,0,0,1], 8081)));
}

pub fn set_proxy_addr(addr: &str) {
    *PROXY_ADDR.lock().unwrap() = addr.parse().unwrap();
}

pub async fn default_handler(mut ctx: HttpContext) -> Result<Response> {
    let url_str = format!("http://{}{}",
        *PROXY_ADDR.lock().unwrap(),
        ctx.req.uri().path_and_query().map(|v| v.as_str()).unwrap_or("/"));
    *ctx.req.uri_mut() = url_str.parse().unwrap();
    let client = PROXY_CLIENT.clone();

    match client.request(ctx.req).await {
        Ok(r) => Ok(r),
        Err(e) => {
            log::error!("反向代理{url_str}错误: {e:?}");
            Err(anyhow::anyhow!("服务未启动"))
        },
    }
}
