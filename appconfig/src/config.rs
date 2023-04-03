use std::borrow::Cow;
use std::fmt::Display;
use std::fs;
use std::path::Path;
use std::str::FromStr;

macro_rules! skip_chars {
    (@sub $c: expr, $ch: literal) => {
        if $c != $ch { break; }
    };
    (@sub $c: expr, $ch1: literal, $ch2: literal) => {
        if $c != $ch1 && $c != $ch2 { break; }
    };
    (@sub $c: expr, $ch1: literal, $ch2: literal, $ch3: literal) => {
        if $c != $ch1 && $c != $ch2 && $c != $ch3 { break; }
    };
    (@sub $c: expr, $ch1: literal, $ch2: literal, $ch3: literal, $ch4: literal) => {
        if $c != $ch1 && $c != $ch2 && $c != $ch3 && $c != $ch4 { break; }
    };

    ($val: expr, $pos: expr, $max: expr, $($t: tt)*) => {
        while $pos < $max {
            let c = $val[$pos];
            skip_chars!(@sub c, $($t)*);
            $pos += 1;
        }
    };
}

struct ConfigItem {
    key_begin: usize,
    key_end: usize,
    val_begin: usize,
    val_end: usize,
}

impl ConfigItem {
    fn new() -> Self { Self {key_begin: 0, key_end: 0, val_begin: 0, val_end: 0} }
}

/// Config Struct
pub struct Config {
    data: Vec<u8>,
    kv: Vec<ConfigItem>,
}

/// Config Implementation
impl Config {
    pub fn with_file<T: AsRef<Path>>(file: T) -> anyhow::Result<Self> {
        let data = fs::read(file)?;
        let kv = Self::parse(&data)?;
        Ok(Self {data, kv})
    }

    pub fn with_text(text: String) -> anyhow::Result<Self> {
        let data = text.into_bytes();
        let kv = Self::parse(&data)?;
        Ok(Self {data, kv})
    }

    pub fn with_data(data: Vec<u8>) -> anyhow::Result<Self> {
        let kv = Self::parse(&data)?;
        Ok(Self {data, kv})
    }

    /// Get a value from config as ayn type (That Impls str::FromStr)
    pub fn get<T>(&self, key: &str) -> anyhow::Result<Option<T>>
            where T: FromStr, T::Err: Display {
        match self.get_raw(key) {
            Some(s) => {
                match Self::decode(s)?.parse::<T>() {
                    Ok(v) => Ok(Some(v)),
                    Err(e) => return Err(anyhow::anyhow!("can't parse value error: {e}")),
                }
            },
            None => Ok(None),
        }
    }

    /// Get a value from config as a String
    pub fn get_str(&self, key: &str) -> anyhow::Result<Option<String>> {
        match self.get_raw(key) {
            Some(s) => Ok(Some(Self::decode(s)?.into_owned())),
            None => Ok(None),
        }
    }

    /// Get a value as original data (not escape)
    pub fn get_raw<'a>(&'a self, key: &str) -> Option<&'a [u8]> {
        let key = key.as_bytes();
        for kv in self.kv.iter() {
            if key == &self.data[kv.key_begin..kv.key_end] {
                return Some(&self.data[kv.val_begin .. kv.val_end]);
            }
        }
        None
    }

    // decode value
    fn decode<'a>(val: &'a [u8]) -> anyhow::Result<Cow<'a, str>> {
        // 判断字符串是否有转义字符
        if val.contains(&b'\\') {
            let mut v = Vec::with_capacity(val.len() + 32);
            let (mut i, imax) = (0, val.len());
            while i < imax {
                match val[i] {
                    b'\\' => {
                        i += 1;
                        if i < imax {
                            let c = val[i];
                            // 行尾的'\'，是连接符，跳过下一行的回车换行空白符并继续处理
                            if c == b'\r' || c == b'\n' {
                                skip_chars!(val, i, imax, b'\r', b'\n');
                                skip_chars!(val, i, imax, b' ', b'\t');
                                i -= 1;
                            } else {
                                v.push(Self::escape(c)?);
                            }
                        }
                    },
                    // 回车换行标志变量内容读取结束
                    b'\r' | b'\n' => break,
                    c => v.push(c),
                }
                i += 1;
            }
            Ok(Cow::Owned(String::from_utf8(v)?))
        } else {
            Ok(Cow::Borrowed(std::str::from_utf8(val)?))
        }
    }

    fn escape(v: u8) -> anyhow::Result<u8> {
        let c = match v {
            b't' => b'\t',
            b'r' => b'\r',
            b'n' => b'\n',
            b'\\' => b'\\',
            _ => anyhow::bail!("The escape character format is not supported \\{v}"),
        };
        Ok(c)
    }

    /// Parse a string into the config
    fn parse(data: &[u8]) -> anyhow::Result<Vec<ConfigItem>> {
        enum ParseStatus { KeyBegin, Comment, Key, Equal, ValBegin, Val, ValContinue, ValComment }

        let mut result = Vec::with_capacity(64);
        let mut pstate = ParseStatus::KeyBegin;
        let mut curr = ConfigItem::new();
        let (mut i, imax, mut line_no) = (0, data.len(), 1);

        macro_rules! push_str {
            ($vec: expr, $item: expr, $pos: expr, $state: expr, $next_state: expr) => {
                $state = $next_state;
                $item.val_end = $pos;
                $vec.push($item);
                $item = ConfigItem::new();
            };
        }

        while i < imax {
            let c = data[i];
            if c == b'\n' { line_no += 1 };

            match pstate {
                ParseStatus::KeyBegin => {
                    match c {
                        b'#' => pstate = ParseStatus::Comment,
                        b'=' => anyhow::bail!("Not allow start with '=' at line {line_no}"),
                        b' ' | b'\t' | b'\r' | b'\n' => {},
                        _ => {
                            pstate = ParseStatus::Key;
                            curr.key_begin = i;
                        }
                    }
                },
                ParseStatus::Comment => {
                    match c {
                        b'\r' | b'\n' => pstate = ParseStatus::KeyBegin,
                        _ => {},
                    }
                },
                ParseStatus::Key => {
                    match c {
                        b' ' | b'\t' => {
                            pstate = ParseStatus::Equal;
                            curr.key_end = i;
                        },
                        b'=' => {
                            pstate = ParseStatus::ValBegin;
                            curr.key_end = i;
                        },
                        b'\r' | b'\n' | b'#' => anyhow::bail!("Not found field value in line {line_no}"),
                        _ => {},
                    }
                },
                ParseStatus::Equal => {
                    match c {
                        b'=' => pstate = ParseStatus::ValBegin,
                        b' ' | b'\t' => {},
                        _ => anyhow::bail!("Not found '=' in line {line_no}, {i}"),
                    }
                },
                ParseStatus::ValBegin => {
                    match c {
                        b'\r' | b'\n' | b'#' => {
                            let s = if c != b'#' { ParseStatus::KeyBegin } else { ParseStatus::ValComment };
                            push_str!(result, curr, 0, pstate, s);
                        },
                        b' ' | b'\t' => {},
                        _ => {
                            pstate = ParseStatus::Val;
                            curr.val_begin = i;
                        },
                    }
                },
                ParseStatus::Val => {
                    match c {
                        b'\r' | b'\n' | b' ' | b'\t' => {
                            if data[i - 1] == b'\\' {
                                pstate = ParseStatus::ValContinue;
                            } else {
                                push_str!(result, curr, i, pstate, ParseStatus::KeyBegin);
                            }
                        },
                        b'#' => {
                            push_str!(result, curr, i, pstate, ParseStatus::ValComment);
                        },
                        _ => {},
                    }
                },
                ParseStatus::ValContinue => {
                    match c {
                        b'\r' | b'\n' | b' ' | b'\t' => {},
                        _ => {
                            pstate = ParseStatus::Val;
                        }
                    }
                },
                ParseStatus::ValComment => {
                    match c {
                        b'\r' | b'\n' => pstate = ParseStatus::KeyBegin,
                        _ => {},
                    }
                },
            }
            i += 1;
        }

        match pstate {
            ParseStatus::ValBegin => result.push(curr),
            ParseStatus::Val | ParseStatus::ValContinue => {
                curr.val_end = imax;
                result.push(curr);
            },
            ParseStatus::Key | ParseStatus::Equal => anyhow::bail!("Not found value at line {line_no}"),
            _ => {},
        }

        Ok(result)
    }

}

impl Default for Config {
    fn default() -> Config {
        Config {data: Vec::with_capacity(0), kv: Vec::with_capacity(0)}
    }
}
