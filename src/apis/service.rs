use anyhow::Result;
use httpserver::{HttpContext, ResBuiler, Response};
use serde::{Serialize, Deserialize};

use crate::aidb;
use crate::date_format;

pub async fn ping(ctx: HttpContext) -> Result<Response> {
    #[derive(Deserialize)] struct ReqParam { reply: Option<String> }

    #[derive(Serialize)]
    struct ResData {
        reply: String,
        server: String,
        #[serde(with = "date_format")]
        now: chrono::DateTime<chrono::Local>,
    }

    let now = chrono::Local::now();
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
    struct ResData {
        token: String,
        expire: chrono::DateTime<chrono::Local>,
    }

    let req_param = ctx.into_json::<ReqParam>().await?;
    httpserver::check_required!(req_param, user, pass);
    let ac = crate::AppConf::get();
    let fpath = std::path::Path::new(&ac.database);
    let username = fpath.file_stem().unwrap();

    if !fpath.exists() || username.to_str().unwrap() != req_param.user.unwrap().as_str() {
        return ResBuiler::fail("无效的用户名");
    }
    if !crate::aidb::check_password(&ac.database, &req_param.pass.unwrap())? {
        return ResBuiler::fail("无效的密码")
    }

    ResBuiler::ok(&ResData {
        token: "12345678".to_string(),
        expire: chrono::Local::now(),
    })
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

    let req_param = ctx.into_json::<ReqParam>().await?;
    let ac = crate::AppConf::get();
    let recs = crate::aidb::load_database(&ac.database, &ac.password)?;
    let mut ret = ResData { total: 0, records: Vec::with_capacity(recs.len()) };

    for item in recs.iter() {
        if let Some(q) = &req_param.q {
            if item.title.contains(q) || item.url.contains(q) || item.notes.contains(q) {
                ret.records.push(item.clone());
            }
        } else {
            ret.records.push(item.clone());
        }
    }
    ret.total = ret.records.len();

    ResBuiler::ok(&ret)
}
