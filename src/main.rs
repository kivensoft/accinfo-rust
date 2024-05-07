mod apis;
mod aidb;

use httpserver::HttpServer;
use tokio::time;

macro_rules! arg_err {
    ($text:literal) => {
        concat!("arg ", $text, " format error")
    };
}

/// 应用程序内部名称
const APP_NAME: &str = include_str!(concat!(env!("OUT_DIR"), "/.app_name"));

/// app版本号, 来自编译时由build.rs从cargo.toml中读取的版本号(读取内容写入.version文件)
const APP_VER: &str = include_str!(concat!(env!("OUT_DIR"), "/.version"));

const BANNER: &str = r#"
  kivensoft %      _       ____
  ____ ___________(_)___  / __/___
 / __ `/ ___/ ___/ / __ \/ /_/ __ \
/ /_/ / /__/ /__/ / / / / __/ /_/ /
\__,_/\___/\___/_/_/ /_/_/  \____/
"#;

appconfig::appglobal_define!(app_global, AppGlobal,
    startup_time  : u64,
    task_interval : u64, // 定时任务执行时间间隔（单位：秒）
    cache_expire  : u64, // 数据缓存存活最大有效时间（单位：秒）
    session_expire: u64, // session过期时间（单位：秒）
);

appconfig::appconfig_define!(app_conf, AppConf,
    log_level     : String => ["L", "log-level",      "LogLevel",       "log level(trace/debug/info/warn/error/off)"],
    log_file      : String => ["F", "log-file",       "LogFile",        "log filename"],
    log_max       : String => ["M", "log-max",        "LogFileMaxSize", "log file max size (unit: k/m/g)"],
    no_console    : bool   => ["",  "no-console",     "NoConsole",      "prohibit outputting logs to the console"],
    threads       : String => ["t", "threads",        "Threads",        "set tokio runtime worker threads"],
    listen        : String => ["l", "listen",         "Listen",         "http service ip:port"],
    no_root       : bool   => ["",  "no-root",        "NoRoot",         "disabled auto redirect / to /index.html"],
    database      : String => ["d", "database",       "Database",       "set aidb database filename"],
    password      : String => ["p", "password",       "Password",       "encrypt database with password"],
    encrypt       : String => ["",  "encrypt",        "Encrypt",        "encrypt KeePass xml file to aidb database format"],
    task_interval : String => ["",  "task-interval",  "TaskInterval",   "timed task time interval(unit: second)"],
    cache_expire  : String => ["",  "cache-expire",   "CacheExpire",    "maximum effective time for data cache survival"],
    session_expire: String => ["",  "session-expire", "SessionExpire",  "session expiration time"],
);

impl Default for AppConf {
    fn default() -> AppConf {
        AppConf {
            log_level:      String::from("info"),
            log_file:       String::with_capacity(0),
            log_max:        String::from("10m"),
            no_console:     false,
            threads:        String::from("1"),
            listen:         String::from("0.0.0.0:8888"),
            no_root:        false,
            database:       String::with_capacity(0),
            password:       String::with_capacity(0),
            encrypt:        String::with_capacity(0),
            task_interval:  String::from("180"),
            cache_expire:   String::from("600"),
            session_expire: String::from("1800"),
        }
    }
}

fn init() -> bool {
    let version = format!("{APP_NAME} version {APP_VER} CopyLeft Kivensoft 2023.");
    let ac = AppConf::init();
    if !appconfig::parse_args(ac, &version).expect("parse args fail") {
        return false;
    }

    if ac.database.is_empty() {
        eprintln!("must use --database set aidb database filename");
        return false;
    }

    AppGlobal::init(AppGlobal {
        startup_time: localtime::unix_timestamp(),
        task_interval: ac.task_interval.parse().expect(arg_err!("task_interval")),
        cache_expire: ac.cache_expire.parse().expect(arg_err!("cache_expire")),
        session_expire: ac.session_expire.parse().expect(arg_err!("session_expire")),
    });

    if !ac.listen.is_empty() && ac.listen.as_bytes()[0] == b':' {
        ac.listen.insert_str(0, "0.0.0.0");
    };

    let log_level = asynclog::parse_level(&ac.log_level).expect(arg_err!("log-level"));
    let log_max = asynclog::parse_size(&ac.log_max).expect(arg_err!("log-max"));

    if log_level == log::Level::Trace {
        println!("config setting: {ac:#?}\n");
    }

    asynclog::init_log(log_level, ac.log_file.clone(), log_max,
        !ac.no_console, true).expect("init log error");
    asynclog::set_level("mio".to_owned(), log::LevelFilter::Info);
    asynclog::set_level("want".to_owned(), log::LevelFilter::Info);

    if !ac.encrypt.is_empty() {
        if ac.password.is_empty() {
            eprintln!("must use --password set database password");
            return false;
        }
        aidb::encrypt_database(&ac.encrypt, &ac.password, &ac.database).unwrap();
        println!("{} -> {} conversion completed.", ac.encrypt, ac.database);
        return false;
    }

    if let Some((s1, s2)) = BANNER.split_once('%') {
        let s2 = &s2[APP_VER.len() - 1..];
        let banner = format!("{s1}{APP_VER}{s2}");
        appconfig::print_banner(&banner, true);
    }

    true
}

fn main() {
    if !init() { return; }

    let mut srv = HttpServer::new();
    srv.set_content_path("/api");
    srv.set_default_handler(apis::default_handler);
    srv.set_middleware(httpserver::AccessLog);
    srv.set_middleware(apis::Authentication);

    httpserver::register_apis!(srv, "",
        "ping": apis::ping,
        "login": apis::login,
        "logout": apis::logout,
        "list": apis::list,
    );

    let async_fn = async move {
        let (mut interval, cache_expire) = {
            let ag = AppGlobal::get();
            let interval = time::interval(std::time::Duration::from_secs(ag.task_interval));
            (interval, ag.cache_expire)
        };
        // 启动定时任务
        tokio::spawn(async move {
            interval.tick().await;
            loop {
                interval.tick().await;
                aidb::recycle_cache(std::time::Duration::from_secs(cache_expire));
                apis::Authentication::recycle();
            }
        });

        // 运行http server主服务
        let addr: std::net::SocketAddr = AppConf::get().listen.parse().unwrap();
        srv.run(addr).await.unwrap();
    };

    let ac = AppConf::get();
    let threads = ac.threads.parse::<usize>().expect(arg_err!("threads"));

    #[cfg(not(feature = "multi_thread"))]
    let mut builder = {
        assert!(threads == 1, "{APP_NAME} current version unsupport multi-threads");
        tokio::runtime::Builder::new_current_thread()
    };

    #[cfg(feature = "multi_thread")]
    let mut builder = {
        assert!(threads <= 256, "multi-threads range in 0-256");

        let mut builder = tokio::runtime::Builder::new_multi_thread();
        if threads > 0 {
            builder.worker_threads(threads);
        }

        builder
    };

    builder.enable_all()
        .build()
        .unwrap()
        .block_on(async_fn)

}
