use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serializer, Deserializer};

const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

#[allow(dead_code)]
pub fn serialize<S>(date: &DateTime<Local>, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer, {
    serializer.serialize_str(&format!("{}", date.format(FORMAT)))
}

#[allow(dead_code)]
pub fn deserialize<'de, D>( deserializer: D,) -> Result<DateTime<Local>, D::Error>
        where D: Deserializer<'de>, {
    Local.datetime_from_str(&String::deserialize(deserializer)?, FORMAT).map_err(serde::de::Error::custom)
}
