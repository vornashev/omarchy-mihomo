//! Search the core's GeoSite database (`GeoSite.dat`, v2ray protobuf) by
//! category name and by the domains inside, so a user who knows a site
//! but not the category can still find it.
//!
//! ```text
//! GeoSiteList { repeated GeoSite entry = 1; }
//! GeoSite     { string country_code = 1; repeated Domain domain = 2; }
//! Domain      { Type type = 1; string value = 2; ... }   // 0 keyword, 1 regex, 2 domain, 3 full
//! ```

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Category {
    pub name: String,
    pub size: usize,
    /// Domains matching the query, else the first few.
    pub samples: Vec<String>,
    /// The query matched the category name (not only a domain inside).
    pub by_name: bool,
}

/// `GeoSite.dat` in the core's home directory, any letter case.
pub fn find(home: &Path) -> Option<PathBuf> {
    std::fs::read_dir(home).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case("geosite.dat")))
}

pub fn search(data: &[u8], query: &str, limit: usize) -> Vec<Category> {
    let q = query.trim().to_ascii_lowercase();
    let mut out = Vec::new();
    let mut reader = Reader(data);
    while let Some((field, value)) = reader.field() {
        let (1, Value::Bytes(site)) = (field, value) else { continue };
        if let Some(c) = category(site, &q) {
            out.push(c);
        }
    }
    // Name matches first (exact, then prefix), then by fewest domains: a
    // focused category beats a huge one that merely contains the site.
    out.sort_by_key(|c| (!c.by_name, c.name != q, !c.name.starts_with(&q), c.size, c.name.clone()));
    out.truncate(limit);
    out
}

fn category(site: &[u8], q: &str) -> Option<Category> {
    let mut r = Reader(site);
    let mut name = String::new();
    let mut size = 0;
    let mut samples = Vec::new();
    let mut matched = Vec::new();
    while let Some((field, value)) = r.field() {
        match (field, value) {
            (1, Value::Bytes(b)) => name = String::from_utf8_lossy(b).to_ascii_lowercase(),
            (2, Value::Bytes(d)) => {
                size += 1;
                let (kind, value) = domain(d);
                if kind == 1 {
                    continue;
                }
                if samples.len() < 4 {
                    samples.push(value.to_owned());
                }
                if !q.is_empty() && matched.len() < 4 && value.contains(q) {
                    matched.push(value.to_owned());
                }
            }
            _ => {}
        }
    }
    let by_name = q.is_empty() || name.contains(q);
    if !by_name && matched.is_empty() {
        return None;
    }
    Some(Category { name, size, samples: if matched.is_empty() { samples } else { matched }, by_name })
}

fn domain(d: &[u8]) -> (u64, &str) {
    let mut r = Reader(d);
    let (mut kind, mut value) = (0, "");
    while let Some((field, v)) = r.field() {
        match (field, v) {
            (1, Value::Varint(k)) => kind = k,
            (2, Value::Bytes(b)) => value = std::str::from_utf8(b).unwrap_or_default(),
            _ => {}
        }
    }
    (kind, value)
}

enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Other,
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn varint(&mut self) -> Option<u64> {
        let mut n = 0u64;
        for shift in (0..64).step_by(7) {
            let (&b, rest) = self.0.split_first()?;
            self.0 = rest;
            n |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Some(n);
            }
        }
        None
    }

    /// Next `(field number, value)`; `None` at the end or on corrupt data.
    fn field(&mut self) -> Option<(u64, Value<'a>)> {
        if self.0.is_empty() {
            return None;
        }
        let key = self.varint()?;
        let value = match key & 7 {
            0 => Value::Varint(self.varint()?),
            1 => {
                self.0 = self.0.get(8..)?;
                Value::Other
            }
            2 => {
                let len = usize::try_from(self.varint()?).ok()?;
                let (b, rest) = (self.0.get(..len)?, self.0.get(len..)?);
                self.0 = rest;
                Value::Bytes(b)
            }
            5 => {
                self.0 = self.0.get(4..)?;
                Value::Other
            }
            _ => return None,
        };
        Some((key >> 3, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(field: u8, b: &[u8]) -> Vec<u8> {
        let mut v = vec![field << 3 | 2, b.len() as u8];
        v.extend_from_slice(b);
        v
    }

    fn site(name: &str, domains: &[(u8, &str)]) -> Vec<u8> {
        let mut s = bytes(1, name.as_bytes());
        for (kind, value) in domains {
            let mut d = vec![1 << 3, *kind];
            d.extend(bytes(2, value.as_bytes()));
            s.extend(bytes(2, &d));
        }
        bytes(1, &s)
    }

    #[test]
    fn finds_by_name_and_by_domain() {
        let mut db = site("GOOGLE", &[(2, "google.com"), (2, "ytimg.com"), (2, "gstatic.com"), (2, "google.ru")]);
        db.extend(site("YOUTUBE", &[(2, "youtube.com"), (2, "ytimg.com"), (1, "^yt.*")]));
        db.extend(site("STEAM", &[(3, "store.steampowered.com")]));
        let r = search(&db, "ytimg", 10);
        assert_eq!(r.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["youtube", "google"], "smaller category first");
        assert_eq!(r[0].samples, ["ytimg.com"]);
        assert_eq!(r[0].size, 3);
        let r = search(&db, "you", 10);
        assert_eq!(r[0].name, "youtube");
        assert!(r[0].by_name);
        assert_eq!(search(&db, "", 10).len(), 3);
        assert!(search(&db[..db.len() - 3], "steam", 10).is_empty(), "truncated data is ignored, not a panic");
    }

    /// `OMM_GEOSITE=/path/to/geosite.dat cargo test real_database -- --nocapture`
    #[test]
    fn real_database() {
        let Ok(path) = std::env::var("OMM_GEOSITE") else { return };
        let data = std::fs::read(path).unwrap();
        let t = std::time::Instant::now();
        let r = search(&data, "ytimg", 5);
        eprintln!("{:?} in {:?}", r.iter().map(|c| (&c.name, c.size)).collect::<Vec<_>>(), t.elapsed());
        assert!(r.iter().any(|c| c.name == "youtube"));
    }
}
