//! Profile index (`profiles.yaml`) and per-profile override files.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::{Paths, read_optional, write_atomic};
use crate::seq::{SeqField, SeqOverride};
use crate::yaml::{Mapping, parse_mapping};

/// Scope name of the overrides applied to every profile.
pub const GLOBAL: &str = "global";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Index {
    pub current: Option<String>,
    pub items: Vec<Profile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub source: Source,
    /// Unix seconds of the last successful download or import.
    #[serde(default)]
    pub updated: Option<u64>,
    /// Refresh period; the subscription's `profile-update-interval` if it sent one.
    #[serde(default)]
    pub interval_hours: Option<u32>,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub etag: Option<String>,
    /// Last failed update, cleared on success.
    #[serde(default)]
    pub last_error: Option<String>,
    /// A local profile created empty for the user to fill in an editor;
    /// cleared by the first save that holds a config.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub draft: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    Remote {
        url: String,
        #[serde(default)]
        user_agent: Option<String>,
    },
    Local,
}

/// From the `subscription-userinfo` header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    /// Unix seconds; absent for subscriptions without expiry.
    #[serde(default)]
    pub expire: Option<u64>,
}

/// Body of a new local profile: comments only, so it parses as empty.
pub fn template(name: &str) -> String {
    format!(
        "# name: {name}\n\
         # Paste a mihomo config below and save the file: the profile becomes\n\
         # usable right away. The name line above may be edited or removed.\n\n"
    )
}

/// The name a local profile file gives itself, from a leading
/// `# name: ...` line (before any YAML content).
pub fn name_hint(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .take_while(|l| l.is_empty() || l.starts_with('#'))
        .find_map(|l| l.strip_prefix('#')?.trim_start().strip_prefix("name:"))
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
}

impl Profile {
    pub fn is_local(&self) -> bool {
        matches!(self.source, Source::Local)
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.source, Source::Remote { .. })
    }

    /// True when a remote profile is older than its refresh period.
    pub fn is_due(&self, default_hours: u32, now: u64) -> bool {
        let hours = self.interval_hours.unwrap_or(default_hours);
        self.is_remote() && hours > 0 && self.updated.is_none_or(|t| now.saturating_sub(t) >= u64::from(hours) * 3600)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Overrides {
    pub nodes: crate::nodes::Managed,
    pub merge: Option<Mapping>,
    pub rules: SeqOverride,
    pub proxies: SeqOverride,
    pub groups: SeqOverride,
}

impl Overrides {
    pub fn seq(&self, field: SeqField) -> &SeqOverride {
        match field {
            SeqField::Rules => &self.rules,
            SeqField::Proxies => &self.proxies,
            SeqField::ProxyGroups => &self.groups,
        }
    }

    /// Load `merge.yaml`, `rules.yaml`, `proxies.yaml`, `groups.yaml` of a
    /// scope; missing files mean "no override".
    pub fn load(paths: &Paths, scope: &str) -> Result<Self> {
        let dir = paths.overrides(scope);
        let merge = match read_optional(&dir.join("merge.yaml"))? {
            Some(text) => Some(parse_mapping(&text, &format!("{scope}/merge.yaml"))?).filter(|m| !m.is_empty()),
            None => None,
        };
        let seq = |file: &str| -> Result<SeqOverride> {
            let path = dir.join(file);
            match read_optional(&path)? {
                Some(text) if !text.trim().is_empty() => {
                    // `null` (a file of comments) is an empty override.
                    let v: Option<SeqOverride> = serde_yaml_ng::from_str(&text).map_err(|e| Error::yaml(format!("{scope}/{file}"), e))?;
                    Ok(v.unwrap_or_default())
                }
                _ => Ok(SeqOverride::default()),
            }
        };
        Ok(Self { nodes: crate::nodes::Managed::load(paths, scope)?, merge, rules: seq("rules.yaml")?, proxies: seq("proxies.yaml")?, groups: seq("groups.yaml")? })
    }
}

pub struct Store {
    paths: Paths,
    pub index: Index,
}

impl Store {
    pub fn open(paths: Paths) -> Result<Self> {
        let index = match read_optional(&paths.profiles_index())? {
            Some(text) => serde_yaml_ng::from_str(&text).map_err(|e| Error::yaml("profiles.yaml", e))?,
            None => Index::default(),
        };
        Ok(Self { paths, index })
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn save(&self) -> Result<()> {
        let text = serde_yaml_ng::to_string(&self.index).map_err(|e| Error::yaml("profiles.yaml", e))?;
        write_atomic(&self.paths.profiles_index(), text.as_bytes())
    }

    pub fn get(&self, id: &str) -> Result<&Profile> {
        self.index.items.iter().find(|p| p.id == id).ok_or_else(|| Error::NoProfile(id.into()))
    }

    pub fn get_mut(&mut self, id: &str) -> Result<&mut Profile> {
        self.index.items.iter_mut().find(|p| p.id == id).ok_or_else(|| Error::NoProfile(id.into()))
    }

    pub fn current(&self) -> Option<&Profile> {
        let id = self.index.current.as_deref()?;
        self.index.items.iter().find(|p| p.id == id)
    }

    pub fn text(&self, id: &str) -> Result<String> {
        let path = self.paths.profile(id);
        std::fs::read_to_string(&path).map_err(|e| Error::io(path, e))
    }

    /// Store a profile body, after checking it parses.
    pub fn write_text(&self, id: &str, text: &str) -> Result<()> {
        parse_mapping(text, id)?;
        write_atomic(&self.paths.profile(id), text.as_bytes())
    }

    /// Register a profile; the first usable one becomes current.
    pub fn insert(&mut self, profile: Profile) {
        if self.index.current.is_none() && !profile.draft {
            self.index.current = Some(profile.id.clone());
        }
        self.index.items.push(profile);
    }

    pub fn remove(&mut self, id: &str) -> Result<Profile> {
        let pos = self.index.items.iter().position(|p| p.id == id).ok_or_else(|| Error::NoProfile(id.into()))?;
        let p = self.index.items.remove(pos);
        if self.index.current.as_deref() == Some(id) {
            self.index.current = self.index.items.first().map(|p| p.id.clone());
        }
        let _ = std::fs::remove_file(self.paths.profile(id));
        let _ = std::fs::remove_dir_all(self.paths.overrides(id));
        Ok(p)
    }

    /// Register an empty local profile and write its template, for the user
    /// to fill in an editor.
    pub fn create_draft(&mut self, name: &str) -> Result<Profile> {
        let id = self.new_id();
        write_atomic(&self.paths.profile(&id), template(name).as_bytes())?;
        let profile = Profile {
            id,
            name: name.into(),
            source: Source::Local,
            updated: None,
            interval_hours: None,
            usage: None,
            etag: None,
            last_error: None,
            draft: true,
        };
        self.insert(profile.clone());
        Ok(profile)
    }

    pub fn select(&mut self, id: &str) -> Result<()> {
        if self.get(id)?.draft {
            return Err(Error::Invalid(format!("profile {id} is empty: paste a config into it and save")));
        }
        self.index.current = Some(id.into());
        Ok(())
    }

    /// A short id unique within this store.
    pub fn new_id(&self) -> String {
        let mut n = now() as u32;
        loop {
            let id = format!("p{n:08x}");
            if !self.index.items.iter().any(|p| p.id == id) {
                return id;
            }
            n = n.wrapping_add(1);
        }
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        paths.ensure().unwrap();
        (dir, Store::open(paths).unwrap())
    }

    fn local(id: &str) -> Profile {
        Profile { id: id.into(), name: id.into(), source: Source::Local, updated: None, interval_hours: None, usage: None, etag: None, last_error: None, draft: false }
    }

    #[test]
    fn index_roundtrip_and_current() {
        let (_d, mut s) = store();
        s.insert(local("a"));
        s.insert(local("b"));
        assert_eq!(s.index.current.as_deref(), Some("a"));
        s.select("b").unwrap();
        s.save().unwrap();
        let mut s = Store::open(s.paths().clone()).unwrap();
        assert_eq!(s.current().unwrap().id, "b");
        s.remove("b").unwrap();
        assert_eq!(s.index.current.as_deref(), Some("a"));
        assert!(s.select("zzz").is_err());
    }

    #[test]
    fn rejects_invalid_profile_text() {
        let (_d, s) = store();
        assert!(s.write_text("x", "proxies: [").is_err());
        s.write_text("x", "proxies: []\n").unwrap();
        assert_eq!(s.text("x").unwrap(), "proxies: []\n");
    }

    #[test]
    fn overrides_load_with_missing_and_comment_only_files() {
        let (_d, s) = store();
        let dir = s.paths().overrides("p1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rules.yaml"), "# template\n").unwrap();
        std::fs::write(dir.join("groups.yaml"), "prepend:\n  - {name: G, type: select, proxies: [DIRECT]}\n").unwrap();
        std::fs::write(dir.join("merge.yaml"), "# nothing\n").unwrap();
        let o = Overrides::load(s.paths(), "p1").unwrap();
        assert!(o.rules.is_empty() && o.proxies.is_empty() && o.merge.is_none());
        assert_eq!(o.groups.prepend.len(), 1);
    }

    #[test]
    fn drafts_are_not_selected() {
        let (_d, mut s) = store();
        let d = s.create_draft("New").unwrap();
        assert!(d.draft && s.index.current.is_none());
        assert!(s.select(&d.id).is_err());
        let text = s.text(&d.id).unwrap();
        assert!(parse_mapping(&text, "t").unwrap().is_empty(), "template parses as empty");
        assert_eq!(name_hint(&text).as_deref(), Some("New"));
        s.insert(local("a"));
        assert_eq!(s.index.current.as_deref(), Some("a"));
    }

    #[test]
    fn name_hint_only_in_leading_comments() {
        assert_eq!(name_hint("\n#  name:  Home \nproxies: []\n").as_deref(), Some("Home"));
        assert_eq!(name_hint("proxies: []\n# name: Late\n"), None);
        assert_eq!(name_hint("# name:\n"), None);
    }

    #[test]
    fn due_profiles() {
        let mut p = local("a");
        assert!(!p.is_due(12, 100_000), "local never due");
        p.source = Source::Remote { url: "u".into(), user_agent: None };
        assert!(p.is_due(12, 100_000), "never fetched");
        p.updated = Some(100_000);
        assert!(!p.is_due(12, 100_000 + 3600));
        assert!(p.is_due(12, 100_000 + 12 * 3600));
        p.interval_hours = Some(0);
        assert!(!p.is_due(12, u64::MAX), "0 disables auto update");
    }
}
