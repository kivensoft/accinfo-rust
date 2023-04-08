mod apis;
mod aidb;

use httpserver::HttpServer;
use tokio::time;

const APP_NAME: &str = "accinfo";   // 应用程序内部名称
const APP_VER: &str = "1.2.2";      // 应用程序版本
const SCHEDULED_SECS: u64 = 180;    // 定时任务执行时间间隔（单位：秒）
const CACHE_EXPIRE_SECS: u64 = 600; // 数据缓存存活最大有效时间（单位：秒）
const SESS_EXP_SECS: i64 = 30 * 60; // session过期时间（单位：秒）

const BANNER: &str = r#"
                   _       ____
  ____ _kivensoft_(_)___  / __/___
 / __ `/ ___/ ___/ / __ \/ /_/ __ \
/ /_/ / /__/ /__/ / / / / __/ /_/ /
\__,_/\___/\___/_/_/ /_/_/  \____/
"#;

appconfig::appconfig_define!(AppConf,
    log_level: String => ["L",  "log-level", "LogLevel",       "log level(trace/debug/info/warn/error/off)"],
    log_file : String => ["F",  "log-file",  "LogFile",        "log filename"],
    log_max  : String => ["M",  "log-max",   "LogFileMaxSize", "log file max size(unit: k/m/g)"],
    listen   : String => ["l",  "listen",    "Listen",         "http service ip:port"],
    no_root  : bool   => ["",   "no-root",   "NoRoot",         "disabled auto redirect / to /index.html"],
    database : String => ["d",  "database",  "Database",       "set aidb database filename"],
    password : String => ["p",  "password",  "Password",       "encrypt database with password"],
    encrypt  : String => ["",   "encrypt",   "Encrypt",        "encrypt KeePass xml file to aidb database format"],
);

impl Default for AppConf {
    fn default() -> AppConf {
        AppConf {
            log_level: String::from("info"),
            log_file : String::new(),
            log_max  : String::from("10m"),
            listen   : String::from("0.0.0.0:8080"),
            no_root  : false,
            database : String::new(),
            password : String::new(),
            encrypt  : String::new(),
        }
    }
}

fn init() -> Option<()> {
    let version = format!("{APP_NAME} version {APP_VER} CopyLeft Kivensoft 2021-2023.");
    let ac = AppConf::init();
    if !appconfig::parse_args(ac, &version).unwrap() {
        return None;
    }
    if ac.database.is_empty() {
        eprintln!("must use --database set aidb database filename");
        return None;
    }

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
        .use_async(false)
        .builder()
        .expect("init log failed");

    if !ac.encrypt.is_empty() {
        if ac.password.is_empty() {
            eprintln!("must use --password set database password");
            return None;
        }
        aidb::encrypt_database(&ac.encrypt, &ac.password, &ac.database).unwrap();
        println!("{} -> {} conversion completed.", ac.encrypt, ac.database);
        return None;
    }

    if ac.listen.len() > 0 && ac.listen.as_bytes()[0] == b':' {
        ac.listen.insert_str(0, "0.0.0.0");
    };

    appconfig::print_banner(BANNER, true);

    return Some(());
}

// #[tokio::main(worker_threads = 4)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let None = init() { return; }

    let mut srv = HttpServer::new(true);

    srv.default_handler(apis::default_handler);
    srv.middleware(apis::Authentication);

    httpserver::register_apis!(srv, "/api",
        "/ping": apis::ping,
        "/login": apis::login,
        "/list": apis::list,
    );

    let mut interval = time::interval(std::time::Duration::from_secs(SCHEDULED_SECS));
    tokio::spawn(async move {
        interval.tick().await;
        loop {
            interval.tick().await;
            aidb::recycle_cache(std::time::Duration::from_secs(CACHE_EXPIRE_SECS));
            apis::Authentication::recycle();
        }
    });

    let addr: std::net::SocketAddr = AppConf::get().listen.parse().unwrap();
    srv.run(addr).await.unwrap();

}
