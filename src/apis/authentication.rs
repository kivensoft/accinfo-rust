use std::{
    collections::HashMap, net::Ipv4Addr,
    sync::{atomic::{AtomicU64, Ordering}, OnceLock}
};

use anyhow_ext::{bail, Result};
use parking_lot::Mutex;
use httpserver::{HttpContext, Resp, Response, Next};

use crate::AppGlobal;

pub struct Authentication;

type Sessions = HashMap<u64, u64>; // key: id, value: exp
type CurrentLimitings = HashMap<u32, u32>; // key: ipv4, value: count
type GlobalValue<T> = OnceLock<Mutex<T>>;

const AUTHORIZATION: &str = "Authorization";
const SESSION: &str = "session ";
const MAX_CURRENT_LIMITING: u32 = 3;

/// 限流统计时间(当前分钟)，1分钟变更1次，按分钟限流
static STATIS_TIME: AtomicU64 = AtomicU64::new(0);
/// 当前登录用户的session
static SESSIONS: GlobalValue<Sessions> = OnceLock::new();
/// 当前访问统计，用于限流
static CURRENT_LIMITINGS: GlobalValue<CurrentLimitings> = OnceLock::new();


impl Authentication {
    pub fn recycle() {
        let now = localtime::unix_timestamp();
        let mut sessions = get_sessions().lock();
        let old_len = sessions.len();
        // 删除过期项
        sessions.retain(|_, v| *v > now);
        if old_len > sessions.len() {
            log::trace!("recycle {} session item", old_len - sessions.len());
        }
    }

    fn check_session(id: u64) -> bool {
        let mut sessions = get_sessions().lock();
        let now = localtime::unix_timestamp();
        if let Some(exp) = sessions.get_mut(&id) {
            if *exp > now {
                *exp = now + AppGlobal::get().session_expire;
                return true;
            }
        }

        false
    }

    fn require_authentication(path: &str) -> bool {
        path.starts_with("/api/") && path != "/api/ping"
                && path != "/api/login" && path != "/api/logout"
    }

    pub fn session_id() -> Result<String> {
        const MAX_TRY: u16 = 10_000;

        let mut sessions = get_sessions().lock();
        let mut id = rand::random::<u64>();
        let mut count = 0;

        loop {
            if !sessions.contains_key(&id) { break; }
            id = rand::random();
            if count >= MAX_TRY {
                bail!("create session id has maximum try");
            }
            count += 1;
        }

        let exp = localtime::unix_timestamp() + AppGlobal::get().session_expire;
        sessions.insert(id, exp);

        Ok(format!("{:016x}", id))
    }

    fn check_limit(ip: Ipv4Addr) -> bool {
        let now = localtime::unix_timestamp();
        let now_minute = now / 60;
        let statis_time = STATIS_TIME.load(Ordering::Acquire);

        let mut limits = get_current_limitings().lock();

        // 每隔1秒钟，重新计算限流值
        if now_minute > statis_time {
            STATIS_TIME.store(now_minute, Ordering::Release);
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
                if let Some(session) = auth.strip_prefix(SESSION) {
                    if let Ok(id) = u64::from_str_radix(session, 16) {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    pub fn remove_session_id(ctx: &HttpContext) {
        if let Some(id) = Self::get_session_id(ctx) {
            get_sessions().lock().remove(&id);
        }
    }

}

#[async_trait::async_trait]
impl httpserver::HttpMiddleware for Authentication {
    async fn handle<'a>(&'a self, ctx: HttpContext, next: Next<'a>) -> Result<Response> {
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

fn get_sessions() -> &'static Mutex<Sessions> {
    SESSIONS.get_or_init(|| Mutex::new(Sessions::new()))
}

fn get_current_limitings() -> &'static Mutex<CurrentLimitings> {
    CURRENT_LIMITINGS.get_or_init(|| Mutex::new(CurrentLimitings::new()))
}
