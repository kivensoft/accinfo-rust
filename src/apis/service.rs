use anyhow::Result;
use httpserver::{HttpContext, ResBuiler, Response, LocalDateTime};
use serde::{Serialize, Deserialize};

use crate::aidb;

const ISSUER: &str = "accinfo";
const EXP_SECS: i64 = 30 * 60;

pub async fn ping(ctx: HttpContext) -> Result<Response> {
    #[derive(Deserialize)] struct ReqParam { reply: Option<String> }

    #[derive(Serialize)]
    struct ResData {
        reply: String,
        server: String,
        now: LocalDateTime,
    }

    let now = LocalDateTime::now();
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
        expire: LocalDateTime,
    }

    let req_param = ctx.into_json::<ReqParam>().await?;
    httpserver::check_required!(req_param, user, pass);
    let (user, pass) = (req_param.user.unwrap(), req_param.pass.unwrap());
    let ac = crate::AppConf::get();
    let fpath = std::path::Path::new(&ac.database);
    let username = fpath.file_stem().unwrap();

    if !fpath.exists() || username.to_str().unwrap() != &user {
        return ResBuiler::fail("无效的用户名");
    }
    if !crate::aidb::check_password(&ac.database, &pass)? {
        return ResBuiler::fail("无效的密码")
    }

    let token = jwt::encode_with_rsa(&serde_json::json!({"user": user}), ISSUER, EXP_SECS as u64)?;
    let expire = LocalDateTime::from(chrono::Local::now() + chrono::Duration::seconds(EXP_SECS));

    ResBuiler::ok(&ResData { token, expire })
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
