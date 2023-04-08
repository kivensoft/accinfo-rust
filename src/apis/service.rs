use std::{sync::Arc, path::Path};
use anyhow::Result;
use chrono::{Local, Duration};
use httpserver::{HttpContext, ResBuiler, Response, LocalTime};
use serde::{Serialize, Deserialize};
use parking_lot::Mutex;
use crate::{aidb, apis::authentication::Authentication, SESS_EXP_SECS};

static PASSWORD: Mutex<String> = Mutex::new(String::new());

pub async fn ping(ctx: HttpContext) -> Result<Response> {
    #[derive(Deserialize)] struct ReqParam { reply: Option<String> }

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

    let now = LocalTime::now();
    let server = format!("{}/{}", crate::APP_NAME, crate::APP_VER);
    let reply = match ctx.into_option_json::<ReqParam>().await? {
        Some(ping_params) => ping_params.reply,
        None => None,
    }.unwrap_or("pong".to_owned());

    ResBuiler::ok(&ResData { reply, now, server })
}

pub async fn login(ctx: HttpContext) -> Result<Response> {
    #[derive(Deserialize)]
    struct ReqParam {
        user: Option<String>,
        pass: Option<String>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ResData {
        token: String,
        expire: LocalTime,
        refresh_time: LocalTime,
    }

    let req_param = ctx.into_json::<ReqParam>().await?;
    let (user, pass) = httpserver::assign_required!(req_param, user, pass);

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
    let expire = LocalTime::from(Local::now() + Duration::seconds(SESS_EXP_SECS));
    let refresh_time = LocalTime::from(Local::now() + Duration::seconds(SESS_EXP_SECS / 2));

    ResBuiler::ok(&ResData { token, expire, refresh_time })
}


pub async fn list(ctx: HttpContext) -> Result<Response> {
    #[derive(Deserialize)]
    struct ReqParam {
        q: Option<String>,
    }

    #[derive(Serialize)]
    struct ResData {
        total: usize,
        records: aidb::Records,
    }

    let req_param = ctx.into_option_json::<ReqParam>().await?;
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
    ResBuiler::ok(&ResData{records: Arc::from(vec_record), total})
}
