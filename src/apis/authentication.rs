use std::{time::SystemTime, net::Ipv4Addr, collections::HashMap};
use parking_lot::Mutex;

use httpserver::{HttpContext, Resp, Response, Next};

use crate::AppGlobal;

pub struct Authentication;

type Sessions = HashMap<u64, u64>; // key: id, value: exp
type CurrentLimitings = HashMap<u32, u32>; // key: ipv4, value: count

const AUTHORIZATION: &str = "Authorization";
const SESSION: &str = "session ";
const MAX_CURRENT_LIMITING: u32 = 3;

static mut STATIS_TIME: u64 = 0; // 限流统计时间，1分钟变更1次，按分钟限流

lazy_static::lazy_static! {
    static ref SESSIONS: Mutex<Sessions> = Mutex::new(HashMap::new());
    static ref CURRENT_LIMITINGS: Mutex<CurrentLimitings> = Mutex::new(HashMap::new());
}


impl Authentication {
    pub fn recycle() {
        let mut sessions = SESSIONS.lock();
        let now = Self::now();
        let len = sessions.len();
        sessions.retain(|_, v| *v > now);
        if len > sessions.len() {
            log::trace!("recycle {} session item", len - sessions.len());
        }
    }

    fn check_session(id: u64) -> bool {
        let mut sessions = SESSIONS.lock();
        let now = Self::now();
        if let Some(exp) = sessions.get_mut(&id) {
            if *exp > now {
                *exp = now + AppGlobal::get().session_expire;
                return true;
            }
        }

        false
    }

    fn require_authentication(path: &str) -> bool {
        return path.starts_with("/api/") && path != "/api/ping"
                && path != "/api/login" && path != "/api/logout"
    }

    fn now() -> u64 {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                .expect("Unable to calculate unix epoch").as_secs()
    }

    pub fn session_id() -> anyhow::Result<String> {
        const MAX_TRY: usize = 10_000;

        let mut sessions = SESSIONS.lock();
        let mut id = rand::random::<u64>();
        let mut count = 0;

        loop {
            if !sessions.contains_key(&id) { break; }
            id = rand::random();
            if count >= MAX_TRY {
                anyhow::bail!("create session id has maximum try");
            }
            count += 1;
        }

        let exp = Self::now() + AppGlobal::get().session_expire;
        sessions.insert(id, exp);

        Ok(format!("{:016x}", id))
    }

    fn check_limit(ip: Ipv4Addr) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("can't get unix epoch of Datetime")
            .as_secs();
        let statis_time = unsafe { STATIS_TIME };

        let mut limits = CURRENT_LIMITINGS.lock();

        // 每隔1秒钟，重新计算限流值
        if now > statis_time {
            unsafe { STATIS_TIME = now };
            limits.clear();
        }

        let ip: u32 = ip.into();
        let visit_count = limits.entry(ip).or_insert(0);
        *visit_count += 1;

        *visit_count <= MAX_CURRENT_LIMITING
    }

    fn get_session_id(ctx: &HttpContext) -> Option<u64> {
        if let Some(auth) = ctx.req.headers().get(AUTHORIZATION) {
            if let Ok(auth) = auth.to_str() {
                if auth.starts_with(SESSION) {
                    if let Ok(id) = u64::from_str_radix(&auth[SESSION.len()..], 16) {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    pub fn remove_session_id(ctx: &HttpContext) {
        if let Some(id) = Self::get_session_id(ctx) {
            SESSIONS.lock().remove(&id);
        }
    }

}

#[async_trait::async_trait]
impl httpserver::HttpMiddleware for Authentication {
    async fn handle<'a>(&'a self, ctx: HttpContext, next: Next<'a>) -> anyhow::Result<Response> {
        if !Self::require_authentication(ctx.req.uri().path()) {
            return next.run(ctx).await
        }

        if let Some(id) = Self::get_session_id(&ctx) {
            // 限流校验
            if Self::check_limit(ctx.remote_ip()) {
                // 登录校验
                if Self::check_session(id) {
                    return next.run(ctx).await
                }
            }
        }

        Resp::fail_with_status(hyper::StatusCode::UNAUTHORIZED,
            hyper::StatusCode::UNAUTHORIZED.as_u16() as u32,
            hyper::StatusCode::UNAUTHORIZED.as_str())
    }
}
