use std::{fmt::{Display, Formatter}, str::FromStr};
use chrono::{Local, DateTime, TimeZone};
use serde::{Serialize, Deserialize, Serializer, Deserializer, de::Visitor};

pub const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub mod datetime_format {
    use chrono::{Local, DateTime, TimeZone};
    use serde::{Deserialize, Serializer, Deserializer};

    pub fn serialize<S>(date: &DateTime<Local>, serializer: S) -> Result<S::Ok, S::Error>
            where S: Serializer, {
        log::debug!("serialze data = {}", date.format(super::DATETIME_FORMAT));
        serializer.serialize_str(&format!("{}", date.format(super::DATETIME_FORMAT)))
    }

    pub fn deserialize<'de, D>( deserializer: D,) -> Result<DateTime<Local>, D::Error>
            where D: Deserializer<'de>, {
        Local.datetime_from_str(&String::deserialize(deserializer)?, super::DATETIME_FORMAT).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone)]
pub struct LocalDateTime(DateTime<Local>);

impl LocalDateTime {
    pub fn now() -> Self {
        LocalDateTime(Local::now())
    }
}

impl Default for LocalDateTime {
    fn default() -> Self {
        Self(DateTime::<Local>::default())
    }
}

impl Display for LocalDateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.format(DATETIME_FORMAT))
    }
}

impl AsRef<DateTime<Local>> for LocalDateTime {
    fn as_ref(&self) -> &DateTime<Local> {
        &self.0
    }
}

impl AsMut<DateTime<Local>> for LocalDateTime {
    fn as_mut(&mut self) -> &mut DateTime<Local> {
        &mut self.0
    }
}

impl From<DateTime<Local>> for LocalDateTime {
    fn from(value: DateTime<Local>) -> Self {
        LocalDateTime(value)
    }
}

impl FromStr for LocalDateTime {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Local.datetime_from_str(s, DATETIME_FORMAT) {
            Ok(v) => Ok(Self(v)),
            Err(e) => anyhow::bail!(e),
        }
    }
}

impl Serialize for LocalDateTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl <'de> Deserialize<'de> for LocalDateTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error> {
        deserializer.deserialize_str(LocalDateTimeVisitor)         // 为 Deserializer 提供 Visitor
    }
}

struct LocalDateTimeVisitor; // LocalDateTime 的 Visitor，用来反序列化

impl <'de> Visitor<'de> for LocalDateTimeVisitor {
    type Value = LocalDateTime; // Visitor 的类型参数，这里我们需要反序列化的最终目标是 LocalDateTime

    // 必须重写的函数，用于为预期之外的类型提供错误信息
    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("datetime format must is yyyy-MM-dd HH:mm:ss")
    }

    // 从字符串中反序列化
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        match Local.datetime_from_str(v, DATETIME_FORMAT) {
            Ok(t) => Ok(LocalDateTime(t)),
            Err(_) => Err(E::custom("datetime format must is yyyy-MM-dd HH:mm:ss")),
        }
    }
}
