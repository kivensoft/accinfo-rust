mod localtime;
pub use localtime::{LocalTime, datetime_format, DATETIME_FORMAT};

use std::sync::Arc;
use hyper::body::Buf;
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use anyhow::Result;

/// Batch registration API interface
/// ## Example
/// ```rust
/// use anyhow::Result;
/// use httpserver::{HttpContext, Response, register_apis};
///
/// async fn ping(ctx: HttpContext) -> Result<Response> { todo!() }
/// async fn login(ctx: HttpContext) -> Result<Response> { todo!() }
///
/// let mut srv = HttpServer::new(true);
/// register_apis!(srv, "/api",
///     "/ping": apis::ping,
///     "/login": apis::login,
/// );
/// ```
#[macro_export]
macro_rules! register_apis {
    ($server:expr, $base:literal, $($path:literal : $handler:expr,)+) => {
        $($server.register(String::from(concat!($base, $path)), $handler); )*
    };
}

/// Error message response returned when struct fields is Option::None
/// ## Example
/// ```rust
/// use httpserver::check_required;
///
/// struct User {
///     name: Option<String>,
///     age: Option<u8>,
/// }
///
/// let user = User { name: None, age: 48 };
///
/// check_required!(user, name, age);
/// ```
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

/// Error message response returned when struct fields is Option::None
/// ## Example
/// ```rust
/// use httpserver::assign_required;
///
/// struct User {
///     name: Option<String>,
///     age: Option<u8>,
/// }
///
/// let user = User { name: String::from("kiven"), age: 48 };
///
/// let (name, age) = assign_required!(user, name, age);
///
/// assert_eq!("kiven", name);
/// assert_eq!(48, age);
/// ```
#[macro_export]
macro_rules! assign_required {
    ($val:expr, $($attr:tt),+) => {
        {
            $(
                if $val.$attr.is_none() {
                    return $crate::ResBuiler::fail(&format!("{} can't be null", stringify!($attr)));
                }
            )*
            ( $( &$val.$attr.unwrap(),)* )
        }
    };
}

/// Error message response returned when expression is true
/// ## Example
/// ```rust
/// use httpserver::fail_if;
///
/// let age = 30;
/// fail_if!(age >= 100, "age must be range 1..100");
/// fail_if!(age >= 100, "age is {}, not in range 1..100", age);
/// ```
#[macro_export]
macro_rules! fail_if {
    ($b:expr, $msg:literal) => {
        if $b {
            return $crate::ResBuiler::fail($msg);
        }
    };
    ($b:expr, $($t:tt)+) => {
        if $b {
            return $crate::ResBuiler::fail(&format!($($t)*));
        }
    };
}

/// Conditional assignment, similar to the ternary operator
///
///  ## Example
/// ```rust
/// use httpserver::assign_if;
///
/// let a = assign_if!(true, 52, 42);
/// let b = assign_if!(false, 52, 42);
/// assert_eq(52, a);
/// assert_eq(42, b);
/// ```
#[macro_export]
macro_rules! assign_if {
    ($b:expr, $val1:expr, $val2:expr) => {
        if $b { $val1 } else { $val2 }
    };
}


/// API interface returns data format
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

    /// Asynchronous parsing of the body content of HTTP requests in JSON format
    ///
    /// Returns:
    ///
    /// **Ok(val)**: body isn't empty and parse success, **Err(e)**: parse error
    ///
    ///  ## Example
    /// ```rust
    /// use anyhow::Result;
    /// use httpserver::{HttpContext, Response, ResBuiler};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct ReqParam {
    ///     user: Option<String>,
    ///     pass: Option<String>,
    /// }
    ///
    /// async fn ping(ctx: HttpContext) -> Result<Response> {
    ///     let req_param = ctx.into_json::<ReqParam>().await?;
    ///     ResBuiler::ok_with_empty()
    /// }
    /// ```
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

    /// Asynchronous parsing of the body content of HTTP requests in JSON format,
    ///
    /// Returns:
    ///
    /// **Ok(None)**: body is empty, **Ok(Some(val))**: body isn't empty, **Err(e)**: parse error
    ///
    ///  ## Example
    /// ```rust
    /// use anyhow::Result;
    /// use httpserver::{HttpContext, Response, ResBuiler};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct ReqParam {
    ///     user: Option<String>,
    ///     pass: Option<String>,
    /// }
    ///
    /// async fn ping(ctx: HttpContext) -> Result<Response> {
    ///     let req_param = ctx.into_option_json::<ReqParam>().await?;
    ///     ResBuiler::ok_with_empty()
    /// }
    /// ```
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

    /// Create a reply message with the specified status code and content
    ///
    /// Arguments:
    ///
    /// * `status`: http status code
    /// * `body`: http response body
    ///
    /// # Examples
    ///
    /// ```
    /// use httpserver::ResBuilder;
    ///
    /// ResBuiler::resp(hyper::StatusCode::Ok, hyper::Body::from(format!("{}",
    ///     serde_json::json!({
    ///         "code": 200,
    ///             "data": {
    ///                 "name":"kiven",
    ///                 "age": 48,
    ///             },
    ///     })
    /// ))?;
    /// ````
    pub fn resp(status: hyper::StatusCode, body: hyper::Body) -> Result<Response> {
        hyper::Response::builder()
            .status(status)
            .header("Content-Type", "applicatoin/json; charset=UTF-8")
            .body(body)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Create a reply message with 200
    ///
    /// # Examples
    ///
    /// ```
    /// use httpserver::ResBuilder;
    ///
    /// ResBuiler::resp_ok(hyper::Body::from(format!("{}",
    ///     serde_json::json!({
    ///         "code": 200,
    ///             "data": {
    ///                 "name":"kiven",
    ///                 "age": 48,
    ///             },
    ///     })
    /// ))?;
    /// ````
    pub fn resp_ok(body: hyper::Body) -> Result<Response> {
        hyper::Response::builder()
            .header("Content-Type", "applicatoin/json; charset=UTF-8")
            .body(body)
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Create a reply message with 200, response body is empty
    pub fn ok_with_empty() -> Result<Response> {
        Self::resp_ok(hyper::Body::from(r#"{"code":200}"#))
    }

    /// Create a reply message with 200
    ///
    /// # Examples
    ///
    /// ```
    /// use httpserver::ResBuilder;
    ///
    /// ResBuiler::ok(&serde_json::json!({
    ///     "code": 200,
    ///         "data": {
    ///             "name":"kiven",
    ///             "age": 48,
    ///         },
    /// }))?;
    /// ````
    pub fn ok<T: ?Sized + Serialize>(data: &T) -> Result<Response> {
        Self::resp_ok(to_json_body!({"code": 200, "data": data}))
    }

    /// Create a reply message with http status 500
    ///
    /// # Examples
    ///
    /// ```
    /// use httpserver::ResBuilder;
    ///
    /// ResBuiler::fail("required field `username`")?;
    /// ````
    pub fn fail(message: &str) -> Result<Response> {
        Self::fail_with_code(500, message)
    }

    /// Create a reply message with specified error code
    ///
    /// # Examples
    ///
    /// ```
    /// use httpserver::ResBuilder;
    ///
    /// ResBuiler::fail_with_code(10086, "required field `username`")?;
    /// ````
    pub fn fail_with_code(code: u32, message: &str) -> Result<Response> {
        Self::resp_ok(to_json_body!({"code": code, "message": message}))
    }

    /// Create a reply message with specified http status and error code
    ///
    /// # Examples
    ///
    /// ```
    /// use httpserver::ResBuilder;
    ///
    /// ResBuiler::fail_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR,
    ///         10086, "required field `username`")?;
    /// ````
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

/// Log middleware
pub struct AccessLog;

impl AccessLog {
    fn get_remote_ip(ctx: &HttpContext) -> std::net::Ipv4Addr {
        if let Some(ip) = ctx.req.headers().get("X-Real-IP") {
            if let Ok(ip) = ip.to_str() {
                if let Ok(ip) = ip.parse() {
                    return ip;
                }
            }
        }
        match ctx.addr.ip() {
            std::net::IpAddr::V4(ip) => ip,
            _ => std::net::Ipv4Addr::new(0, 0, 0, 0),
        }
    }
}

#[async_trait::async_trait]
impl HttpMiddleware for AccessLog {
    async fn handle<'a>(&'a self, ctx: HttpContext, next: Next<'a>) -> Result<Response> {
        let start = std::time::Instant::now();
        let ip = Self::get_remote_ip(&ctx);
        let method = ctx.req.method().to_string();
        let path = ctx.req.uri().path().to_string();

        let res = next.run(ctx).await;
        let ms = start.elapsed().as_millis();
        match &res {
            Ok(res) => {
                if log::log_enabled!(log::Level::Debug) {
                    let c = if res.status() == hyper::StatusCode::OK { 2 } else { 3 };
                    let c2 = if ms < 100 { 6 } else { 5 };
                    log::debug!("{method} \x1b[34m{path} \x1b[3{c}m{} \x1b[3{c2}m{ms}\x1b[0mms client: {ip}",
                            res.status().as_str());
                }
            },
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

    /// Create a new HttpServer
    ///
    /// Arguments:
    ///
    /// * `use_access_log`: set Log middleware if true
    ///
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

    /// set default function when no matching api function is found
    ///
    /// Arguments:
    ///
    /// * `handler`: The default function when no matching interface function is found
    ///
    pub fn default_handler(&mut self, handler: impl HttpHandler) {
        self.default_handler = Box::new(handler);
    }

    /// register api function for path
    ///
    /// Arguments:
    ///
    /// * `path`: api path
    /// * `handler`: handle of api function
    pub fn register(&mut self, path: String, handler: impl HttpHandler) {
        self.router.insert(path, Box::new(handler));
    }

    /// register middleware
    pub fn middleware(&mut self, middleware: impl HttpMiddleware) {
        self.middlewares.push(Arc::new(middleware));
    }

    /// run http service and enter message loop mode
    ///
    /// Arguments:
    ///
    /// * `addr`: listen addr
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

    pub fn concat_path(path1: &str, path2: &str) -> String {
        let mut s = String::with_capacity(path1.len() + path2.len() + 1);
        s.push_str(path1);
        if s.as_bytes()[s.len() - 1] != b'/' {
            s.push('/');
        }
        let path2 = if path2.as_bytes()[0] != b'/' { path2 } else { &path2[1..] };
        s.push_str(path2);
        return s;
    }
}
