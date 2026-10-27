//! Rule sets the user adds on top of the profile's rule providers, for
//! `RULE-SET` rules. A catalog name (`geosite:openai`, `geoip:ru`) stands
//! for a compact `.mrs` list from MetaCubeX meta-rules-dat: one category,
//! without downloading the whole GeoSite database.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::yaml::{Mapping, Value, get_mut_mapping};

const CATALOG_BASE: &str = "https://github.com/MetaCubeX/meta-rules-dat/raw/meta/geo";

/// Categories worth offering before the user types anything.
pub const POPULAR: &[&str] = &[
    "geosite:openai", "geosite:anthropic", "geosite:google-gemini", "geosite:category-ai-!cn", "geosite:youtube", "geosite:google",
    "geosite:telegram", "geosite:discord", "geosite:twitter", "geosite:instagram", "geosite:facebook", "geosite:tiktok",
    "geosite:netflix", "geosite:spotify", "geosite:github", "geosite:microsoft", "geosite:apple", "geosite:steam",
    "geosite:cloudflare", "geosite:category-ru", "geosite:category-ads-all", "geoip:ru", "geoip:telegram", "geoip:netflix",
    "geoip:cloudflare", "geoip:private",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RuleSet {
    pub name: String,
    pub url: String,
    /// `domain`, `ipcidr` or `classical`.
    pub behavior: String,
    /// `mrs`, `yaml` or `text`.
    pub format: String,
    #[serde(default = "default_interval")]
    pub interval_hours: u32,
    /// Download through this group, for hosts blocked directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
}

fn default_interval() -> u32 {
    24
}

/// `geosite:openai` → `geosite-openai` from meta-rules-dat.
pub fn catalog(spec: &str) -> Option<RuleSet> {
    let (kind, name) = spec.trim().split_once(':')?;
    let name = name.trim().to_ascii_lowercase();
    let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '!' | '@' | '_' | '.'));
    let behavior = match kind.trim().to_ascii_lowercase().as_str() {
        "geosite" => "domain",
        "geoip" => "ipcidr",
        _ => return None,
    };
    let kind = kind.trim().to_ascii_lowercase();
    ok.then(|| RuleSet {
        name: format!("{kind}-{}", name.replace('!', "not-")),
        url: format!("{CATALOG_BASE}/{kind}/{name}.mrs"),
        behavior: behavior.into(),
        format: "mrs".into(),
        interval_hours: default_interval(),
        proxy: None,
    })
}

impl RuleSet {
    pub fn validate(&self) -> Result<()> {
        let bad = |m: String| Err(Error::Invalid(m));
        if self.name.is_empty() || !self.name.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.')) {
            return bad(format!("rule set name {:?}: letters, digits, '-', '_' and '.' only", self.name));
        }
        if !(self.url.starts_with("https://") || self.url.starts_with("http://")) {
            return bad(format!("{}: the URL must be http(s)", self.name));
        }
        if !matches!(self.behavior.as_str(), "domain" | "ipcidr" | "classical") {
            return bad(format!("{}: behavior is domain, ipcidr or classical", self.name));
        }
        if !matches!(self.format.as_str(), "mrs" | "yaml" | "text") {
            return bad(format!("{}: format is mrs, yaml or text", self.name));
        }
        if self.format == "mrs" && self.behavior == "classical" {
            return bad(format!("{}: mrs lists hold domains or networks, not classical rules", self.name));
        }
        Ok(())
    }

    fn provider(&self) -> Value {
        let ext = match self.format.as_str() {
            "text" => "list",
            f => f,
        };
        let mut m = Mapping::new();
        m.insert("type".into(), "http".into());
        m.insert("url".into(), self.url.as_str().into());
        m.insert("behavior".into(), self.behavior.as_str().into());
        m.insert("format".into(), self.format.as_str().into());
        m.insert("interval".into(), (u64::from(self.interval_hours.clamp(1, 24 * 30)) * 3600).into());
        m.insert("path".into(), format!("./ruleset/omm-{}.{ext}", self.name).into());
        if let Some(p) = &self.proxy {
            m.insert("proxy".into(), p.as_str().into());
        }
        Value::Mapping(m)
    }
}

/// Add the user's sets to `rule-providers`. A name the profile already
/// uses stays the profile's: its own rules depend on it.
pub fn apply(sets: &[RuleSet], config: &mut Mapping) {
    if sets.is_empty() {
        return;
    }
    let providers = get_mut_mapping(config, "rule-providers");
    for s in sets {
        providers.entry(s.name.as_str().into()).or_insert_with(|| s.provider());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::parse_mapping;

    #[test]
    fn catalog_names() {
        let s = catalog("geosite:OpenAI").unwrap();
        assert_eq!((s.name.as_str(), s.behavior.as_str(), s.format.as_str()), ("geosite-openai", "domain", "mrs"));
        assert_eq!(s.url, "https://github.com/MetaCubeX/meta-rules-dat/raw/meta/geo/geosite/openai.mrs");
        assert_eq!(catalog("geoip:ru").unwrap().behavior, "ipcidr");
        assert_eq!(catalog("geosite:category-ai-!cn").unwrap().name, "geosite-category-ai-not-cn");
        assert!(catalog("geosite:../x").is_none());
        assert!(catalog("asn:13335").is_none());
        for spec in POPULAR {
            catalog(spec).unwrap().validate().unwrap();
        }
    }

    #[test]
    fn validation() {
        let mut s = catalog("geosite:openai").unwrap();
        s.behavior = "classical".into();
        assert!(s.validate().is_err(), "mrs cannot be classical");
        s.format = "yaml".into();
        s.validate().unwrap();
        s.url = "file:///etc/passwd".into();
        assert!(s.validate().is_err());
        s.url = "https://x".into();
        s.name = "a b".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn applied_without_shadowing_the_profile() {
        let mut c = parse_mapping("rule-providers: {geosite-openai: {type: file, behavior: domain, path: ./p.yaml}}\n", "p").unwrap();
        let mut mine = catalog("geosite:openai").unwrap();
        mine.url = "https://elsewhere".into();
        let yt = RuleSet { proxy: Some("AI".into()), ..catalog("geosite:youtube").unwrap() };
        apply(&[mine, yt], &mut c);
        let p = &c["rule-providers"];
        assert_eq!(p["geosite-openai"]["type"], Value::from("file"), "the profile's provider stays");
        assert_eq!(p["geosite-youtube"]["path"], Value::from("./ruleset/omm-geosite-youtube.mrs"));
        assert_eq!(p["geosite-youtube"]["interval"], Value::from(86400u64));
        assert_eq!(p["geosite-youtube"]["proxy"], Value::from("AI"));
    }
}
