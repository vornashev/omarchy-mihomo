//! Minimal MaxMind DB reader for the core's `ASN.mmdb` and a country
//! database (`Country.mmdb`, MetaCubeX `geoip.metadb`): enough to look an
//! address up and read strings and integers out of the record.
//!
//! ```text
//! [search tree: node_count nodes of 2 records] [16 zero bytes] [data section]
//! ... "\xAB\xCD\xEFMaxMind.com" [metadata map]
//! ```

use std::net::IpAddr;
use std::path::Path;

const META_MARKER: &[u8] = b"\xAB\xCD\xEFMaxMind.com";

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Uint(u64),
    Int(i64),
    Float(f64),
    Bool(bool),
    Map(Vec<(String, Value)>),
    Array(Vec<Value>),
    Bytes,
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Value> {
        keys.iter().try_fold(self, |v, k| v.get(k))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Uint(n) => Some(*n),
            Value::Int(n) if *n >= 0 => Some(*n as u64),
            _ => None,
        }
    }
}

pub struct Reader {
    data: Vec<u8>,
    node_count: usize,
    record_size: usize,
    ip_version: u64,
    tree_size: usize,
    /// Node where the IPv4 subtree (`::/96`) starts in an IPv6 tree.
    ipv4_start: usize,
}

impl Reader {
    pub fn open(path: &Path) -> Option<Self> {
        Self::from_bytes(std::fs::read(path).ok()?)
    }

    pub fn from_bytes(data: Vec<u8>) -> Option<Self> {
        let start = data.windows(META_MARKER.len()).rposition(|w| w == META_MARKER)? + META_MARKER.len();
        let meta = Decoder { buf: &data[start..], base: 0 }.decode(0).ok()?.0;
        let node_count = meta.get("node_count")?.as_u64()? as usize;
        let record_size = meta.get("record_size")?.as_u64()? as usize;
        if !matches!(record_size, 24 | 28 | 32) {
            return None;
        }
        let ip_version = meta.get("ip_version")?.as_u64()?;
        let tree_size = record_size * 2 / 8 * node_count;
        if tree_size + 16 > data.len() {
            return None;
        }
        let mut r = Self { data, node_count, record_size, ip_version, tree_size, ipv4_start: 0 };
        if ip_version == 6 {
            let mut node = 0;
            for _ in 0..96 {
                if node >= node_count {
                    break;
                }
                node = r.record(node, 0);
            }
            r.ipv4_start = node;
        }
        Some(r)
    }

    fn record(&self, node: usize, bit: u8) -> usize {
        let b = &self.data;
        match self.record_size {
            24 => {
                let o = node * 6 + bit as usize * 3;
                (b[o] as usize) << 16 | (b[o + 1] as usize) << 8 | b[o + 2] as usize
            }
            28 => {
                let o = node * 7;
                if bit == 0 {
                    ((b[o + 3] & 0xF0) as usize) << 20 | (b[o] as usize) << 16 | (b[o + 1] as usize) << 8 | b[o + 2] as usize
                } else {
                    ((b[o + 3] & 0x0F) as usize) << 24 | (b[o + 4] as usize) << 16 | (b[o + 5] as usize) << 8 | b[o + 6] as usize
                }
            }
            _ => {
                let o = node * 8 + bit as usize * 4;
                u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize
            }
        }
    }

    pub fn lookup(&self, ip: IpAddr) -> Option<Value> {
        let (bytes, mut node): (Vec<u8>, usize) = match ip {
            IpAddr::V4(v4) => (v4.octets().to_vec(), if self.ip_version == 6 { self.ipv4_start } else { 0 }),
            IpAddr::V6(v6) => {
                if self.ip_version == 4 {
                    return None;
                }
                (v6.octets().to_vec(), 0)
            }
        };
        for i in 0..bytes.len() * 8 {
            if node >= self.node_count {
                break;
            }
            let bit = (bytes[i / 8] >> (7 - i % 8)) & 1;
            node = self.record(node, bit);
        }
        if node <= self.node_count {
            return None;
        }
        let offset = node - self.node_count - 16;
        let section = &self.data[self.tree_size + 16..];
        Decoder { buf: section, base: 0 }.decode(offset).ok().map(|(v, _)| v)
    }
}

struct Decoder<'a> {
    buf: &'a [u8],
    base: usize,
}

type Res<T> = Result<T, ()>;

impl Decoder<'_> {
    fn byte(&self, at: usize) -> Res<u8> {
        self.buf.get(at).copied().ok_or(())
    }

    fn uint(&self, at: usize, size: usize) -> Res<u64> {
        let bytes = self.buf.get(at..at + size).ok_or(())?;
        Ok(bytes.iter().fold(0u64, |a, b| a << 8 | *b as u64))
    }

    /// Decode the value at `at`; returns it and the offset just past it.
    fn decode(&self, at: usize) -> Res<(Value, usize)> {
        let ctrl = self.byte(at)?;
        let mut pos = at + 1;
        let mut kind = ctrl >> 5;
        if kind == 1 {
            let ss = (ctrl >> 3) & 3;
            let v = (ctrl & 7) as usize;
            let p = match ss {
                0 => (v << 8 | self.byte(pos)? as usize, 1),
                1 => ((v << 16 | self.uint(pos, 2)? as usize) + 2048, 2),
                2 => ((v << 24 | self.uint(pos, 3)? as usize) + 526_336, 3),
                _ => (self.uint(pos, 4)? as usize, 4),
            };
            let (value, _) = self.decode(self.base + p.0)?;
            return Ok((value, pos + p.1));
        }
        if kind == 0 {
            kind = 7 + self.byte(pos)?;
            pos += 1;
        }
        let mut size = (ctrl & 0x1f) as usize;
        if kind != 14 {
            size = match size {
                29 => { pos += 1; 29 + self.byte(pos - 1)? as usize }
                30 => { pos += 2; 285 + self.uint(pos - 2, 2)? as usize }
                31 => { pos += 3; 65_821 + self.uint(pos - 3, 3)? as usize }
                n => n,
            };
        }
        match kind {
            2 => {
                let s = self.buf.get(pos..pos + size).ok_or(())?;
                Ok((Value::Str(String::from_utf8_lossy(s).into_owned()), pos + size))
            }
            3 => Ok((Value::Float(f64::from_bits(self.uint(pos, 8)?)), pos + 8)),
            4 => Ok((Value::Bytes, pos + size)),
            5 | 6 | 9 | 10 => {
                let n = if size > 8 { self.uint(pos + size - 8, 8)? } else { self.uint(pos, size)? };
                Ok((Value::Uint(n), pos + size))
            }
            8 => {
                let n = self.uint(pos, size)? as u32;
                let n = if size == 4 { n as i32 as i64 } else { n as i64 };
                Ok((Value::Int(n), pos + size))
            }
            7 => {
                let mut map = Vec::with_capacity(size.min(64));
                for _ in 0..size {
                    let (k, next) = self.decode(pos)?;
                    let (v, next) = self.decode(next)?;
                    pos = next;
                    if let Value::Str(k) = k {
                        map.push((k, v));
                    }
                }
                Ok((Value::Map(map), pos))
            }
            11 => {
                let mut list = Vec::with_capacity(size.min(64));
                for _ in 0..size {
                    let (v, next) = self.decode(pos)?;
                    pos = next;
                    list.push(v);
                }
                Ok((Value::Array(list), pos))
            }
            14 => Ok((Value::Bool(size != 0), pos)),
            15 => Ok((Value::Float(f32::from_bits(self.uint(pos, 4)? as u32) as f64), pos + 4)),
            _ => Err(()),
        }
    }
}

/// ASN and organisation from a GeoLite2-ASN style record.
pub fn asn(v: &Value) -> Option<(u32, String)> {
    let n = v.get("autonomous_system_number")?.as_u64()? as u32;
    let org = v.get("autonomous_system_organization").and_then(Value::as_str).unwrap_or_default().to_owned();
    Some((n, org))
}

/// ISO country code from a MaxMind country record, or from a MetaCubeX
/// `metadb`, whose records are a code or a list of codes.
pub fn country(v: &Value) -> Option<String> {
    let code = match v {
        Value::Str(s) => Some(s.as_str()),
        Value::Array(list) => list.iter().find_map(Value::as_str),
        Value::Map(_) => v.path(&["country", "iso_code"]).or_else(|| v.path(&["registered_country", "iso_code"])).and_then(Value::as_str),
        _ => None,
    }?;
    let code = code.trim().to_ascii_uppercase();
    (code.len() == 2).then_some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny IPv4 tree: one node whose left half (0.0.0.0/1) maps to a
    /// map record and right half to "not found".
    fn tiny() -> Vec<u8> {
        let node_count = 1usize;
        let mut b = Vec::new();
        // record_size 24: left points at data offset 0 => node_count + 16 + 0.
        let left = node_count + 16;
        b.extend_from_slice(&[(left >> 16) as u8, (left >> 8) as u8, left as u8]);
        b.extend_from_slice(&[0, 0, node_count as u8]);
        b.extend_from_slice(&[0; 16]);
        // {"autonomous_system_number": 15169, "autonomous_system_organization": "GOOGLE"}
        b.push(0xE2);
        push_str(&mut b, "autonomous_system_number");
        b.extend_from_slice(&[0xC2, 0x3B, 0x41]);
        push_str(&mut b, "autonomous_system_organization");
        push_str(&mut b, "GOOGLE");
        b.extend_from_slice(META_MARKER);
        b.push(0xE4);
        push_str(&mut b, "node_count");
        b.extend_from_slice(&[0xC1, node_count as u8]);
        push_str(&mut b, "record_size");
        b.extend_from_slice(&[0xC1, 24]);
        push_str(&mut b, "ip_version");
        b.extend_from_slice(&[0xA1, 4]);
        push_str(&mut b, "database_type");
        push_str(&mut b, "Test-ASN");
        b
    }

    fn push_str(b: &mut Vec<u8>, s: &str) {
        if s.len() < 29 {
            b.push(0x40 | s.len() as u8);
        } else {
            b.extend_from_slice(&[0x40 | 29, (s.len() - 29) as u8]);
        }
        b.extend_from_slice(s.as_bytes());
    }

    #[test]
    fn looks_up_tiny_tree() {
        let r = Reader::from_bytes(tiny()).unwrap();
        let v = r.lookup("8.8.8.8".parse().unwrap()).unwrap();
        assert_eq!(asn(&v), Some((15169, "GOOGLE".into())));
        assert!(r.lookup("200.1.1.1".parse().unwrap()).is_none());
    }

    #[test]
    fn country_shapes() {
        assert_eq!(country(&Value::Str("nl".into())), Some("NL".into()));
        assert_eq!(country(&Value::Array(vec![Value::Str("RU".into())])), Some("RU".into()));
        let m = Value::Map(vec![("country".into(), Value::Map(vec![("iso_code".into(), Value::Str("DE".into()))]))]);
        assert_eq!(country(&m), Some("DE".into()));
    }

    /// Real database from the core's home, when present.
    #[test]
    fn real_asn_database() {
        let Some(home) = std::env::var_os("HOME") else { return };
        let path = Path::new(&home).join(".local/share/omarchy-mihomo/runtime/ASN.mmdb");
        let Some(r) = Reader::open(&path) else { return };
        let v = r.lookup("8.8.8.8".parse().unwrap()).unwrap();
        assert_eq!(asn(&v).unwrap().0, 15169);
        let v = r.lookup("1.1.1.1".parse().unwrap()).unwrap();
        assert_eq!(asn(&v).unwrap().0, 13335);
    }

    /// A downloaded country database: `OMM_TEST_COUNTRY=path`.
    #[test]
    fn real_country_database() {
        let Some(path) = std::env::var_os("OMM_TEST_COUNTRY") else { return };
        let r = Reader::open(Path::new(&path)).unwrap();
        let v = r.lookup("77.88.8.8".parse().unwrap()).unwrap();
        assert_eq!(country(&v).as_deref(), Some("RU"));
        let v = r.lookup("5.9.0.1".parse().unwrap()).unwrap();
        assert_eq!(country(&v).as_deref(), Some("DE"));
        // MetaCubeX tags some services instead of a country: not a country.
        if let Some(v) = r.lookup("8.8.8.8".parse().unwrap()) {
            assert_ne!(country(&v).as_deref(), Some("GOOGLE"));
        }
    }
}
