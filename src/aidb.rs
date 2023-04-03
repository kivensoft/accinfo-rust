use std::{io::{Write, Read}, sync::{Arc, Mutex}};
use serde::{Serialize, Deserialize};
use quick_xml::{events::Event, reader::Reader};
use md5::{Md5, Digest, Md5Core, digest::Output};
use aes::cipher::{KeyIvInit, StreamCipher};

type Aes128Ctr64LE = ctr::Ctr64LE<aes::Aes128>;

const IV: &str = "The great rejuvenation of the Chinese nation";

lazy_static::lazy_static! {
    static ref G_RECS: Mutex<Option<CacheRecord>> = Mutex::new(None);
}

pub type Records = Vec<Arc<Record>>;

pub struct CacheRecord {
    pub data: Arc<Records>,
    time: std::time::Instant,
}

pub fn recycle_cache(expire: std::time::Duration) {
    let mut g_recs = G_RECS.lock().unwrap();
    if let Some(recs) = g_recs.as_ref() {
        if recs.time.elapsed() > expire {
            g_recs.take();
            log::trace!("缓存数据闲置时间过长，释放缓存数据占用的内存");
        }
    }
}

struct MyAes (Aes128Ctr64LE);

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

fn load_xml(xml: &[u8]) -> anyhow::Result<Vec<Record>> {
    #[derive(PartialEq, Eq, Debug)]
    enum ElementType { None, Entry, Id, String, Key, Value }
    #[derive(PartialEq, Eq, Debug)]
    enum KeyValueType { None, Title, User, Pass, Url, Notes }

    let mut reader = Reader::from_str(std::str::from_utf8(xml)?);
    let mut recs = Vec::new();
    let mut rec = Record::default();
    let mut e_type = ElementType::None;
    let mut kv_type = KeyValueType::None;
    let mut value = String::new();

    loop {
        match reader.read_event() {
            Ok(event) => match event {
                Event::Start(e) => match e.name().as_ref() {
                    b"Entry" => e_type = ElementType::Entry,
                    b"UUID" if e_type == ElementType::Entry => e_type = ElementType::Id,
                    b"String" if e_type == ElementType::Entry => e_type = ElementType::String,
                    b"Key" if e_type == ElementType::String => e_type = ElementType::Key,
                    b"Value" if e_type == ElementType::String => e_type = ElementType::Value,
                    _ => {},
                },
                Event::End(e) => match e.name().as_ref() {
                    b"Entry" => {
                        if !rec.title.is_empty() {
                            recs.push(rec);
                            rec = Record::default();
                        }
                        e_type = ElementType::None;
                    },
                    b"UUID" if e_type == ElementType::Id => e_type = ElementType::Entry,
                    b"String" if e_type == ElementType::String => {
                        e_type = ElementType::Entry;
                        match kv_type {
                            KeyValueType::Title => rec.title = value,
                            KeyValueType::User => rec.user = value,
                            KeyValueType::Pass => rec.pass = value,
                            KeyValueType::Url => rec.url = value,
                            KeyValueType::Notes => rec.notes = value,
                            KeyValueType::None => {},
                        };
                        kv_type = KeyValueType::None;
                        value = String::new();
                    },
                    b"Key" if e_type == ElementType::Key => e_type = ElementType::String,
                    b"Value" if e_type == ElementType::Value => e_type = ElementType::String,
                    _ => {},
                },
                Event::Text(e) => match e_type {
                    ElementType::Id => rec.id = e.unescape()?.to_string(),
                    ElementType::Key => {
                        match e.unescape()?.as_bytes() {
                            b"Title" => kv_type = KeyValueType::Title,
                            b"UserName" => kv_type = KeyValueType::User,
                            b"Password" => kv_type = KeyValueType::Pass,
                            b"URL" => kv_type = KeyValueType::Url,
                            b"Notes" => kv_type = KeyValueType::Notes,
                            _ => {},
                        };
                    },
                    ElementType::Value => value = e.unescape()?.to_string(),
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

fn md5_check_text(password: &str) -> Output<Md5Core> {
    let mut hash_md5 = Md5::new();
    hash_md5.update(password);
    hash_md5.update(IV);
    hash_md5.finalize()
}

const MAGIC: &[u8] = b"aidb";
const MAGIC_LEN: usize = 4;
const HEADER_LEN: usize = MAGIC_LEN + 4;
const ATTACH_LEN: usize = HEADER_LEN + 16;

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

    let check_data = md5_check_text(password);
    debug_assert!(check_data.len() == ATTACH_LEN - HEADER_LEN);

    let mut ofile = std::fs::File::create(out_file)?;
    ofile.write_all(MAGIC)?;
    ofile.write_all(&recs_json_len)?;
    ofile.write_all(&check_data)?;
    ofile.write_all(&recs_json)?;

    Ok(())
}

/// Load database content using the specified password
///
/// * `aidb`: Database file name
/// * `password`: Database password
pub fn load_database(aidb: &str, password: &str) -> anyhow::Result<Arc<Records>> {
    let mut g_recs = G_RECS.lock().unwrap();
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
    if md5_check_text(password).as_slice() != &buf[HEADER_LEN..ATTACH_LEN] {
        anyhow::bail!("password error");
    }

    aes_decrypt(password.as_bytes(), &mut buf[ATTACH_LEN..]);

    let recs: CacheRecord = CacheRecord {
        data: Arc::new(serde_json::from_slice(&buf[ATTACH_LEN..])?),
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

    if md5_check_text(password).as_slice() != &buf[HEADER_LEN..ATTACH_LEN] {
        return Ok(false);
    }

    Ok(true)
}
