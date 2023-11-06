use std::{sync::Arc, path::Path};
use httpserver::{HttpContext, Resp, HttpResult};
use localtime::LocalTime;
use serde::{Serialize, Deserialize};
use parking_lot::Mutex;
use crate::{aidb, apis::authentication::Authentication, AppGlobal, unix_timestamp};

static PASSWORD: Mutex<String> = Mutex::new(String::new());

pub async fn ping(ctx: HttpContext) -> HttpResult {
    #[derive(Deserialize, Default)] struct ReqParam { reply: Option<String> }

    #[derive(Serialize)]
    struct ResData {
        reply: String,
        server: String,
        now: LocalTime,
    }

    if log::log_enabled!(log::Level::Trace) {
        for entry in ctx.req.headers() {
            log::trace!("[header] {}: {}", entry.0.as_str(),
                    std::str::from_utf8(entry.1.as_bytes()).unwrap());
        }
    }

    let req_param = ctx.into_opt_json::<ReqParam>().await?.unwrap_or_default();
    let reply = req_param.reply.unwrap_or_else(|| "pong".to_owned());
    let now = LocalTime::now();
    let server = format!("{}/{}", crate::APP_NAME, crate::APP_VER);

    Resp::ok(&ResData { reply, now, server })
}

/// 登录接口
pub async fn login(ctx: HttpContext) -> HttpResult {
    #[derive(Deserialize)]
    struct ReqParam {
        user: String,
        pass: String,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ResData {
        token: String,
        expire: LocalTime,
        refresh_time: LocalTime,
    }

    let req_param = ctx.into_json::<ReqParam>().await?;
    let (user, pass) = (&req_param.user, &req_param.pass);

    let ac = crate::AppConf::get();
    let fpath = Path::new(&ac.database);
    let username = fpath.file_stem().unwrap();

    httpserver::fail_if!(!fpath.exists(), "数据库丢失");
    httpserver::fail_if!(username.to_str().unwrap() != user, "用户名错误");
    httpserver::fail_if!(!crate::aidb::check_password(&ac.database, pass)?, "密码错误");

    // 保存用户密码
    let mut p = PASSWORD.lock();
    if pass != p.as_str() {
        *p = String::from(pass);
    }
    drop(p);

    let token = Authentication::session_id()?;
    let now = unix_timestamp() as i64;
    let expire = LocalTime::from_unix_timestamp(now + AppGlobal::get().session_expire as i64);
    let refresh_time = LocalTime::from_unix_timestamp(now + AppGlobal::get().session_expire as i64 / 2);

    Resp::ok(&ResData { token, expire, refresh_time })
}

/// 退出登录接口
pub async fn logout(ctx: HttpContext) -> HttpResult {
    Authentication::remove_session_id(&ctx);
    Resp::ok_with_empty()
}

/// 数据查询接口
pub async fn list(ctx: HttpContext) -> HttpResult {
    #[derive(Deserialize)]
    struct ReqParam {
        q: Option<String>,
    }

    #[derive(Serialize)]
    struct ResData {
        total: usize,
        records: aidb::Records,
    }

    let req_param = ctx.into_opt_json::<ReqParam>().await?;
    let ac = crate::AppConf::get();
    let pass = PASSWORD.lock();
    let recs = crate::aidb::load_database(&ac.database, pass.as_str())?;
    let mut vec_record = Vec::with_capacity(recs.len());
    // let mut ret = ResData { total: 0, records: Vec::with_capacity(recs.len()) };

    let has_q = req_param.is_some() && req_param.as_ref().unwrap().q.is_some();
    let q = httpserver::assign_if!(has_q, req_param.unwrap().q.unwrap(), String::with_capacity(0));

    for item in recs.iter() {
        if has_q {
            if item.title.contains(&q) || item.url.contains(&q) || item.notes.contains(&q) {
                vec_record.push(item.clone());
            }
        } else {
            vec_record.push(item.clone());
        }
    }

    let total = vec_record.len();
    Resp::ok(&ResData{records: Arc::from(vec_record), total})
}
