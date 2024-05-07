use std::{sync::Arc, path::Path};
use httpserver::{HttpContext, HttpResponse, Resp};
use localtime::LocalTime;
use serde::{Serialize, Deserialize};
use parking_lot::Mutex;
use crate::{aidb, apis::authentication::Authentication, AppGlobal};

static PASSWORD: Mutex<String> = Mutex::new(String::new());

pub async fn ping(ctx: HttpContext) -> HttpResponse {
    #[derive(Deserialize, Default)] struct ReqParam { reply: Option<String> }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ResData {
        reply: String,
        server: String,
        now: LocalTime,
        client_ip: String,
    }

    let req_param = ctx.parse_json_opt::<ReqParam>()?.unwrap_or_default();

    Resp::ok(&ResData {
        reply: req_param.reply.unwrap_or_else(|| "pong".to_owned()),
        now: LocalTime::now(),
        server: format!("{}/{}", crate::APP_NAME, crate::APP_VER),
        client_ip: ctx.addr.to_string(),
    })
}

/// 登录接口
pub async fn login(ctx: HttpContext) -> HttpResponse {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
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

    let req_param = ctx.parse_json::<ReqParam>()?;
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
    let now = localtime::unix_timestamp() as i64;
    let expire = LocalTime::from_unix_timestamp(now + AppGlobal::get().session_expire as i64);
    let refresh_time = LocalTime::from_unix_timestamp(now + AppGlobal::get().session_expire as i64 / 2);

    Resp::ok(&ResData { token, expire, refresh_time })
}

/// 退出登录接口
pub async fn logout(ctx: HttpContext) -> HttpResponse {
    Authentication::remove_session_id(&ctx);
    Resp::ok_with_empty()
}

/// 数据查询接口
pub async fn list(ctx: HttpContext) -> HttpResponse {
    #[derive(Deserialize)]
    struct ReqParam {
        q: Option<String>,
    }

    #[derive(Serialize)]
    struct ResData {
        total: usize,
        records: aidb::Records,
    }

    let req_param = ctx.parse_json_opt::<ReqParam>()?;
    let ac = crate::AppConf::get();
    let pass = PASSWORD.lock();
    let recs = crate::aidb::load_database(&ac.database, pass.as_str())?;
    let mut vec_record = Vec::with_capacity(recs.len());

    let q = match req_param {
        Some(rp) => match rp.q {
            Some(q) => q,
            None => String::with_capacity(0),
        }
        None => String::with_capacity(0),
    };

    for item in recs.iter() {
        if !q.is_empty() {
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
