#[macro_use] mod httpserver;
mod apis;

use httpserver::HttpServer;

const BANNER: &str = r#"
   ____ ___  ____ _____  (_)___ _kivensoft
  / __ `__ \/ __ `/ __ \/ / __ `/ | /| / /
 / / / / / / /_/ / /_/ / / /_/ /| |/ |/ /
/_/ /_/ /_/\__,_/ .___/_/\__, / |__/|__/
   ver: 0.9.0  /_/      /____/
"#;

const APP_NAME: &str = "mapigw";
const APP_VER: &str = "0.9.0";

appconfig::appconfig_define!(AppConf,
    log_level: String => ["L",  "log-level", "LogLevel",       "log level(trace/debug/info/warn/error/off)"],
    log_file : String => ["F",  "log-file",  "LogFile",        "log filename"],
    log_max  : String => ["M",  "log-max",   "LogFileMaxSize", "log file max size(unit: k/m/g)"],
    listen   : String => ["l",  "listen",    "Listen",         "http service ip:port"],
    proxy    : String => ["x",  "proxy",     "ProxyAddress",   "reverse proxy ip:port"],
);

impl Default for AppConf {
    fn default() -> AppConf {
        AppConf {
            log_level: String::from("info"),
            log_file : String::new(),
            log_max  : String::from("10m"),
            listen   : String::from("0.0.0.0:8080"),
            proxy    : String::from("127.0.0.1:8081"),
        }
    }
}

fn init() -> Option<Box<AppConf>> {
    let version = format!("{APP_NAME} version {APP_VER} CopyLeft Kivensoft 2023.");
    let mut ac = Box::new(AppConf::default());
    if !appconfig::parse_args(ac.as_mut(), &version).unwrap() {
        return None;
    }

    appconfig::print_banner(BANNER, true);

    let log_level = asynclog::parse_level(&ac.log_level).unwrap();
    let log_max = asynclog::parse_size(&ac.log_max).unwrap();

    if log_level == log::Level::Trace {
        println!("config setting: {ac:#?}\n");
    }

    asynclog::Builder::new()
        .level(log_level)
        .log_file(ac.log_file.clone())
        .log_file_max(log_max)
        .use_console(true)
        .use_async(true)
        .builder()
        .expect("init log failed");

    return Some(ac);
}

// #[tokio::main(worker_threads = 4)]
#[tokio::main(flavor = "current_thread")]
async fn main() {

    let mut ac = match init() {
        Some(val) => val,
        None => return,
    };

    if ac.listen.len() > 0 && ac.listen.as_bytes()[0] == b':' {
        ac.listen.insert_str(0, "0.0.0.0");
    };

    let addr: std::net::SocketAddr = ac.listen.parse().unwrap();
    apis::set_proxy_addr(&ac.proxy);

    let mut srv = HttpServer::new(true);
    srv.default_handler(apis::default_handler);

    httpserver_register!(srv, "",
        "/ping": apis::ping,
    );

    srv.run(addr).await.unwrap();

}
