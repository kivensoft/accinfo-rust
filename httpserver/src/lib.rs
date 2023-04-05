mod localdatetime;
pub use localdatetime::{LocalDateTime, datetime_format};

use std::sync::Arc;
use hyper::body::Buf;
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use anyhow::Result;

#[macro_export]
macro_rules! register_apis {
    ($server:expr, $base:literal, $($path:literal : $handler:expr,)+) => {
        $($server.register(concat!($base, $path), $handler); )*
    };
}

#[macro_export]
macro_rules! decode_json {
    ($ctx:expr, $t:ty) => {
        {
            let whole_body = hyper::body::aggregate(self.req).await?;
            let r: T = serde_json::from_reader(whole_body.reader())?;
        }
    };
}

#[macro_export]
macro_rules! check_required {
    ($val:expr, $($attr:tt),+) => {
        $(
            if $val.$attr.is_none() {
                return $crate::ResBuiler::fail(&format!("{} can't be null", stringify!($attr)));
            }
        )*
    };
}


#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApiResult<T> {
    pub code: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

pub type Request = hyper::Request<hyper::Body>;
pub type Response = hyper::Response<hyper::Body>;

pub struct HttpContext {
    pub req: Request,
    pub addr: std::net::SocketAddr,
    pub id: u16,
}

impl HttpContext {

    pub async fn into_json<T: DeserializeOwned>(self) -> Result<T> {
        let body = hyper::body::aggregate(self.req).await?;
        match serde_json::from_reader(body.reader()) {
            Ok(v) => Ok(v),
            Err(e) => {
                log::info!("decode http body to json error: {e:?}");
                anyhow::bail!("parse request data failed")
            },
        }
    }

    pub async fn into_option_json<T: DeserializeOwned>(self) -> Result<Option<T>> {
        let body = hyper::body::aggregate(self.req).await?;
        if body.remaining() > 0 {
            match serde_json::from_reader(body.reader()) {
                Ok(v) => Ok(Some(v)),
                Err(e) => {
                    log::info!("decode http body to json error: {e:?}");
                    anyhow::bail!("parse request data failed")
                },
            }
        } else {
            Ok(None)
        }
    }

}

macro_rules! to_json_body {
    ($($t:tt)+) => { hyper::Body::from(format!("{}", serde_json::json!($($t)*)))};
}

pub struct ResBuiler;

impl ResBuiler {

    pub fn resp(status: hyper::StatusCode, body: hyper::Body) -> Result<Response> {
        hyper::Response::builder()
            .status(status)
            .header("Content-Type", "applicatoin/json; charset=UTF-8")
            .body(body)
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub fn resp_ok(body: hyper::Body) -> Result<Response> {
        hyper::Response::builder()
            .header("Content-Type", "applicatoin/json; charset=UTF-8")
            .body(body)
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub fn ok_with_empty() -> Result<Response> {
        Self::resp_ok(hyper::Body::from(r#"{"code":200}"#))
    }

    pub fn ok<T: ?Sized + Serialize>(data: &T) -> Result<Response> {
        Self::resp_ok(to_json_body!({"code": 200, "data": data}))
    }

    pub fn fail(message: &str) -> Result<Response> {
        Self::fail_with_code(500, message)
    }

    pub fn fail_with_code(code: u32, message: &str) -> Result<Response> {
        Self::resp_ok(to_json_body!({"code": code, "message": message}))
    }

    pub fn fail_with_status(status: hyper::StatusCode, code: u32, message: &str) -> Result<Response> {
        Self::resp(status, to_json_body!({"code": code, "message": message}))
    }

}

#[async_trait::async_trait]
pub trait HttpHandler: Send + Sync + 'static {
    async fn handle(&self, ctx: HttpContext) -> Result<Response>;
}

type BoxHttpHandler = Box<dyn HttpHandler>;

#[async_trait::async_trait]
impl<FN: Send + Sync + 'static, Fut> HttpHandler for FN
        where
            FN: Fn(HttpContext) -> Fut,
            Fut: std::future::Future<Output = Result<Response>> + Send + 'static, {

    async fn handle(&self, ctx: HttpContext) -> Result<Response> {
        self(ctx).await
    }
}

type Router = std::collections::HashMap<String, BoxHttpHandler>;

#[async_trait::async_trait]
pub trait HttpMiddleware: Send + Sync + 'static {
    async fn handle<'a>(&'a self, ctx: HttpContext, next: Next<'a>) -> Result<Response>;
}

pub struct Next<'a> {
    pub endpoint: &'a dyn HttpHandler,
    pub next_middleware: &'a [Arc<dyn HttpMiddleware>],
}

impl<'a> Next<'a> {
    pub async fn run(mut self, ctx: HttpContext) -> Result<Response> {
        if let Some((current, next)) = self.next_middleware.split_first() {
            self.next_middleware = next;
            current.handle(ctx, self).await
        } else {
            (self.endpoint).handle(ctx).await
        }
    }
}

pub struct AccessLog;

#[async_trait::async_trait]
impl HttpMiddleware for AccessLog {
    async fn handle<'a>(&'a self, ctx: HttpContext, next: Next<'a>) -> Result<Response> {
        let start = std::time::Instant::now();
        let remote_addr = ctx.addr;
        let method = ctx.req.method().to_string();
        let path = ctx.req.uri().path().to_string();

        let res = next.run(ctx).await;
        let ms = start.elapsed().as_millis();
        match &res {
            Ok(res) => log::debug!("{method} \x1b[34m{path} \x1b[33m{} \x1b[36m{ms}\x1b[0mms client: {remote_addr}",
                res.status().as_str()),
            Err(e)  => log::error!("{method} \x1b[34m{path} \x1b[36m{ms}\x1b[0mms error: \x1b[31m{e}\x1b[0m"),
        };

        res
    }
}

pub struct HttpServer {
    router: Router,
    middlewares: Vec<Arc<dyn HttpMiddleware>>,
    default_handler: BoxHttpHandler,
}

impl HttpServer {

    pub fn new(use_access_log: bool) -> Self {
        let mut middlewares: Vec<Arc<dyn HttpMiddleware>> = Vec::new();
        if use_access_log {
            middlewares.push(Arc::new(AccessLog));
        }
        HttpServer {
            router: std::collections::HashMap::new(),
            middlewares,
            default_handler: Box::new(Self::handle_not_found),
        }
    }

    pub fn default_handler(&mut self, handler: impl HttpHandler) {
        self.default_handler = Box::new(handler);
    }

    /// 注册http接口服务
    ///
    /// Arguments:
    ///
    /// * `path`: 接口路径
    /// * `handler`: 接口处理函数
    pub fn register(&mut self, path: &str, handler: impl HttpHandler) {
        self.router.insert(String::from(path), Box::new(handler));
    }

    /// 注册http服务中间件
    ///
    /// Arguments:
    ///
    /// * `middleware`: 中间件对象
    pub fn middleware(&mut self, middleware: impl HttpMiddleware) {
        self.middlewares.push(Arc::new(middleware));
    }

    /// 运行http服务，进入消息循环模式
    ///
    /// Arguments:
    ///
    /// * `addr`: 服务监听地址
    pub async fn run(self, addr: std::net::SocketAddr) -> anyhow::Result<()> {
        use std::convert::Infallible;

        struct ServerData {
            server: HttpServer,
            id: std::sync::atomic::AtomicU16,
        }
        let data = Arc::new(ServerData {
            server: self,
            id: std::sync::atomic::AtomicU16::new(0),
        });

        let make_svc = hyper::service::make_service_fn(|conn: &hyper::server::conn::AddrStream| {
            let data = data.clone();
            let addr = conn.remote_addr();

            async move {
                Ok::<_, Infallible>(hyper::service::service_fn(move |req: Request| {
                    let data = data.clone();

                    async move {
                        let path = req.uri().path().to_owned();
                        let endpoint = match data.server.router.get(&path) {
                            Some(handler) => &**handler,
                            None => data.server.default_handler.as_ref(),
                        };
                        let next = Next { endpoint, next_middleware: &data.server.middlewares };
                        let id = data.id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let ctx = HttpContext { req, addr, id };

                        let resp = match next.run(ctx).await {
                            Ok(resp) => resp,
                            Err(e) => Self::handle_error(e),
                        };

                        Ok::<_, Infallible>(resp)
                    }
                }))
            }
        });

        let server = hyper::Server::bind(&addr).serve(make_svc);
        log::info!("Started http server on \x1b[34m{addr}\x1b[0m");

        server.await.map_err(|e| anyhow::Error::new(e).context("http server running error"))
    }

    async fn handle_not_found(_ctx: HttpContext) -> Result<Response> {
        ResBuiler::fail_with_status(hyper::StatusCode::NOT_FOUND, 404, "Not Found")
    }

    fn handle_error(err: anyhow::Error) -> Response {
        ResBuiler::fail(&err.to_string()).unwrap()
    }

}
