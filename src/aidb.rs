use std::{io::{Write, Read}, sync::Arc};
use serde::{Serialize, Deserialize};
use quick_xml::{events::Event, reader::Reader};
use md5::{Md5, Digest, Md5Core, digest::Output};
use aes::cipher::{KeyIvInit, StreamCipher};
use parking_lot::Mutex;

type Aes128Ctr64LE = ctr::Ctr64LE<aes::Aes128>;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub title: String,
    pub user: String,
    pub pass: String,
    pub url: String,
    pub notes: String,
}

pub type Records = Arc<[Arc<Record>]>;

pub struct CacheRecord {
    pub data: Records,
    time: std::time::Instant,
}

struct MyAes (Aes128Ctr64LE);

const IV: &str = "The great rejuvenation of the Chinese nation";
const MAGIC: &[u8] = b"aidb";
const MAGIC_LEN: usize = 4;
const HEADER_LEN: usize = MAGIC_LEN + 4;
const ATTACH_LEN: usize = HEADER_LEN + 16;

lazy_static::lazy_static! {
    static ref G_RECS: Mutex<Option<CacheRecord>> = Mutex::new(None);
}


pub fn recycle_cache(expire: std::time::Duration) {
    let mut g_recs = G_RECS.lock();
    if let Some(recs) = g_recs.as_ref() {
        if recs.time.elapsed() > expire {
            g_recs.take();
            log::trace!("cache data idle for too long, freeing the memory occupied by cache data");
        }
    }
}

/// Convert the xml file exported from keepass into an aidb database and encrypt it with the specified password
///
/// * `xml_file`: The xml file exported from keepass
/// * `password`: Database password
/// * `out_file`: Output aidb database filename
pub fn encrypt_database(xml_file: &str, password: &str, out_file: &str) -> anyhow::Result<()> {
    let xdata = std::fs::read(xml_file)?;
    let recs = load_xml(&xdata)?;
    log::trace!("{xml_file} record total: {}", recs.len());

    let mut recs_json = serde_json::to_vec(&recs)?;
    aes_encrypt(password.as_bytes(), &mut recs_json);

    let recs_json_len = recs_json.len();
    let recs_json_len = [
        ((recs_json_len >> 24) & 0xff) as u8,
        ((recs_json_len >> 16) & 0xff) as u8,
        ((recs_json_len >>  8) & 0xff) as u8,
        ((recs_json_len      ) & 0xff) as u8,
    ];

    let check_data = &md5_password(password);
    debug_assert!(check_data.len() == ATTACH_LEN - HEADER_LEN);

    let mut ofile = std::fs::File::create(out_file)?;
    ofile.write_all(MAGIC)?;
    ofile.write_all(&recs_json_len)?;
    ofile.write_all(check_data.as_slice())?;
    ofile.write_all(&recs_json)?;

    Ok(())
}

/// Load database content using the specified password
///
/// * `aidb`: Database file name
/// * `password`: Database password
pub fn load_database(aidb: &str, password: &str) -> anyhow::Result<Records> {
    let mut g_recs = G_RECS.lock();
    if let Some(ref mut recs) = *g_recs {
        recs.time = std::time::Instant::now();
        return Ok(recs.data.clone());
    }

    let mut buf = std::fs::read(aidb)?;
    if buf.len() < ATTACH_LEN {
        anyhow::bail!("database size too small");
    }
    if MAGIC != &buf[..MAGIC_LEN] {
        anyhow::bail!("database is not aidb format");
    }
    let len = ((buf[4] as u32) << 24) | ((buf[5] as u32) << 16) | ((buf[6] as u32) << 8) | (buf[7] as u32);
    if (len as usize) != buf.len() - ATTACH_LEN {
        anyhow::bail!("database size format error");
    }
    if md5_password(password).as_slice() != &buf[HEADER_LEN..ATTACH_LEN] {
        anyhow::bail!("password error");
    }

    aes_decrypt(password.as_bytes(), &mut buf[ATTACH_LEN..]);

    let data: Vec<Arc<Record>> = serde_json::from_slice(&buf[ATTACH_LEN..])?;
    let recs: CacheRecord = CacheRecord {
        data: Arc::from(data),
        time: std::time::Instant::now(),
    };

    log::trace!("load database record total: {}", recs.data.len());
    let ret = recs.data.clone();
    *g_recs = Some(recs);

    Ok(ret)
}

/// 校验数据库密码是否正确
///
/// * `aidb`: aidb数据库文件名
/// * `password`: 数据库口令
///
/// Returns:
///
/// Ok(true): 密码正确, Ok(false) 密码错误, Err(e): 其它错误
pub fn check_password(aidb: &str, password: &str) -> anyhow::Result<bool> {
    let mut f = std::fs::File::open(aidb)?;
    let flen = f.metadata()?.len();

    if (flen as usize) < ATTACH_LEN {
        anyhow::bail!("database size too small");
    }

    let mut buf = [0_u8; ATTACH_LEN];
    f.read(&mut buf)?;
    if MAGIC != &buf[..MAGIC_LEN] {
        anyhow::bail!("database is not aidb format");
    }

    let len = ((buf[4] as u32) << 24) | ((buf[5] as u32) << 16) | ((buf[6] as u32) << 8) | (buf[7] as u32);
    if (len as usize) != (flen as usize) - ATTACH_LEN {
        anyhow::bail!("database size format error");
    }

    if md5_password(password).as_slice() != &buf[HEADER_LEN..ATTACH_LEN] {
        return Ok(false);
    }

    Ok(true)
}

impl MyAes {
    pub fn new(key: &[u8]) -> Self {
        let mut hash_md5 = Md5::new();
        hash_md5.update(key);
        let key_md5 = hash_md5.finalize();
        let mut hash_md5 = Md5::new();
        hash_md5.update(IV);
        let iv_md5 = hash_md5.finalize();
        MyAes(Aes128Ctr64LE::new(&key_md5, &iv_md5))
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        self.0.apply_keystream(data);
    }
}

fn load_xml(xml: &[u8]) -> anyhow::Result<Vec<Record>> {
    // xml节点类型
    #[derive(PartialEq, Eq, Debug)]
    enum ElType { None, Entry, Id, String, Key, Value }
    // xml数据节点类型
    #[derive(PartialEq, Eq, Debug)]
    enum KVType { None, Title, User, Pass, Url, Notes }

    let mut reader = Reader::from_str(std::str::from_utf8(xml)?);
    let mut recs = Vec::new();
    let mut rec = Record::default();
    let mut e_type = ElType::None;
    let mut kv_type = KVType::None;
    let mut value = String::new();

    loop {
        match reader.read_event() {
            Ok(event) => match event {
                Event::Start(e) => match e.name().as_ref() {
                    b"Entry" => e_type = ElType::Entry,
                    b"UUID" if e_type == ElType::Entry => e_type = ElType::Id,
                    b"String" if e_type == ElType::Entry => e_type = ElType::String,
                    b"Key" if e_type == ElType::String => e_type = ElType::Key,
                    b"Value" if e_type == ElType::String => e_type = ElType::Value,
                    _ => {},
                },
                Event::End(e) => match e.name().as_ref() {
                    b"Entry" => {
                        if !rec.title.is_empty() {
                            recs.push(rec);
                            rec = Record::default();
                        }
                        e_type = ElType::None;
                    },
                    b"UUID" if e_type == ElType::Id => e_type = ElType::Entry,
                    b"String" if e_type == ElType::String => {
                        e_type = ElType::Entry;
                        match kv_type {
                            KVType::Title => rec.title = value,
                            KVType::User => rec.user = value,
                            KVType::Pass => rec.pass = value,
                            KVType::Url => rec.url = value,
                            KVType::Notes => rec.notes = value,
                            KVType::None => {},
                        };
                        kv_type = KVType::None;
                        value = String::new();
                    },
                    b"Key" if e_type == ElType::Key => e_type = ElType::String,
                    b"Value" if e_type == ElType::Value => e_type = ElType::String,
                    _ => {},
                },
                Event::Text(e) => match e_type {
                    ElType::Id => rec.id = e.unescape()?.to_string(),
                    ElType::Key => {
                        match e.unescape()?.as_bytes() {
                            b"Title" => kv_type = KVType::Title,
                            b"UserName" => kv_type = KVType::User,
                            b"Password" => kv_type = KVType::Pass,
                            b"URL" => kv_type = KVType::Url,
                            b"Notes" => kv_type = KVType::Notes,
                            _ => {},
                        };
                    },
                    ElType::Value => value = e.unescape()?.to_string(),
                    _ => {},
                },
                Event::Eof => break,
                _ => {},
            },
            Err(e) => return Err(anyhow::anyhow!("Error at position {}", reader.buffer_position()).context(e)),
        }
    }

    Ok(recs)
}

fn aes_encrypt(key: &[u8], data: &mut [u8]) {
    let mut cipher = MyAes::new(key);
    cipher.encrypt(data);
}

fn aes_decrypt(key: &[u8], data: &mut [u8]) {
    let mut cipher = MyAes::new(key);
    cipher.encrypt(data);
}

fn md5_password(password: &str) -> Output<Md5Core> {
    let mut hash_md5 = Md5::new();
    hash_md5.update(password);
    hash_md5.update(IV);
    hash_md5.finalize()
}
