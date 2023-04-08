use std::{time::SystemTime, net::Ipv4Addr, collections::HashMap};
use parking_lot::Mutex;

use httpserver::{HttpContext, ResBuiler, Response, Next};

const AUTHORIZATION: &str = "Authorization";
const SESSION: &str = "session ";
const MAX_CURRENT_LIMITING: u32 = 3;
const X_REAL_IP: &str = "X-Real-IP";

struct SessionItem {
    id: u64,
    exp: u64,
}

type CurrentLimitingItem = HashMap<u32, u32>; // key: ipv4, value: count

static SESSIONS: Mutex<Vec<SessionItem>> = Mutex::new(Vec::new());
// 限流统计时间，1分钟变更1次，按分钟限流
static mut STATIS_TIME: u64 = 0;

lazy_static::lazy_static!{
    static ref CURRENT_LIMITINGS: Mutex<CurrentLimitingItem> = Mutex::new(HashMap::new());
}

pub struct Authentication;

impl Authentication {
    pub fn recycle() {
        let mut sessions = SESSIONS.lock();
        let now = Self::now();
        let len = sessions.len();
        sessions.retain(|v| v.exp > now);
        if len > sessions.len() {
            log::trace!("recycle {} session", len - sessions.len());
        }
    }

    fn check_session(id: u64) -> bool {
        let mut sessions = SESSIONS.lock();
        let now = Self::now();
        for item in sessions.as_mut_slice() {
            if item.id == id && item.exp > now {
                item.exp = now + crate::SESS_EXP_SECS as u64;
                return true;
            }
        }
        return false;
    }

    fn require_authentication(path: &str) -> bool {
        return path.starts_with("/api/") && path != "/api/ping" && path != "/api/login"
    }

    fn now() -> u64 {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                .expect("Unable to calculate unix epoch").as_secs()
    }

    pub fn session_id() -> anyhow::Result<String> {
        let mut sessions = SESSIONS.lock();
        let mut id = rand::random::<u64>();
        let mut count = 0;
        loop {
            for item in &*sessions {
                if item.id == id {
                    #[allow(unreachable_code)]
                    if count > 10000 {
                        return anyhow::bail!("generator random session id has reached the maximum number of attempts");
                    }
                    count += 1;
                    id = rand::random();
                    continue;
                }
            }
            break;
        }
        let exp = Self::now() + crate::SESS_EXP_SECS as u64;
        sessions.push(SessionItem {id, exp});

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

    fn get_remote_ip(ctx: &HttpContext) -> Ipv4Addr {
        if let Some(ip) = ctx.req.headers().get(X_REAL_IP) {
            if let Ok(ip) = ip.to_str() {
                if let Ok(ip) = ip.parse() {
                    return ip;
                }
            }
        }
        match ctx.addr.ip() {
            std::net::IpAddr::V4(ip) => ip,
            _ => Ipv4Addr::new(0, 0, 0, 0),
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
            if Self::check_limit(Self::get_remote_ip(&ctx)) {
                // 登录校验
                if Self::check_session(id) {
                    return next.run(ctx).await
                }
            }
        }

        ResBuiler::fail_with_status(hyper::StatusCode::UNAUTHORIZED,
            hyper::StatusCode::UNAUTHORIZED.as_u16() as u32,
            hyper::StatusCode::UNAUTHORIZED.as_str())
    }
}
