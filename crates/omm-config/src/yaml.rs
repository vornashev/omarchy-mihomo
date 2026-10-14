//! Small helpers over `serde_yaml_ng` values.

pub use serde_yaml_ng::{Mapping, Sequence, Value};

use crate::error::{Error, Result};

/// Parse a YAML document that must be a mapping (or empty).
/// Aliases are expanded by the parser; `<<` merge keys are applied here,
/// because the output is re-serialized without anchors.
pub fn parse_mapping(text: &str, what: &str) -> Result<Mapping> {
    let mut value: Value = serde_yaml_ng::from_str(text).map_err(|e| Error::yaml(what, e))?;
    value.apply_merge().map_err(|e| Error::yaml(what, e))?;
    match value {
        Value::Mapping(m) => Ok(m),
        Value::Null => Ok(Mapping::new()),
        _ => Err(Error::Invalid(format!("{what}: expected a mapping at the top level"))),
    }
}

pub fn to_string(map: &Mapping) -> Result<String> {
    serde_yaml_ng::to_string(map).map_err(|e| Error::yaml("generated config", e))
}

/// `name` of a proxy or group entry; bare strings name themselves.
pub fn entry_name(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s),
        Value::Mapping(m) => m.get("name").and_then(Value::as_str),
        _ => None,
    }
}

/// Names of the entries of a top-level sequence (`proxies`, `proxy-groups`).
pub fn names<'a>(config: &'a Mapping, key: &str) -> impl Iterator<Item = &'a str> {
    config
        .get(key)
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(entry_name)
}

pub fn get_mut_mapping<'a>(config: &'a mut Mapping, key: &str) -> &'a mut Mapping {
    let slot = config.entry(Value::from(key)).or_insert_with(|| Value::Mapping(Mapping::new()));
    if !slot.is_mapping() {
        *slot = Value::Mapping(Mapping::new());
    }
    slot.as_mapping_mut().expect("just ensured a mapping")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keys_are_applied() {
        let m = parse_mapping("base: &b {type: vless, udp: true}\nproxies:\n  - name: A\n    <<: *b\n    udp: false\n", "t").unwrap();
        let a = &m["proxies"][0];
        assert_eq!(a["type"], Value::from("vless"));
        assert_eq!(a["udp"], Value::from(false));
        assert!(a.get("<<").is_none());
    }

    #[test]
    fn empty_document_is_empty_mapping() {
        assert!(parse_mapping("# only a comment\n", "t").unwrap().is_empty());
        assert!(parse_mapping("- a\n", "t").is_err());
    }
}
