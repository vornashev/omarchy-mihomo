//! Merge overrides: a YAML mapping deep-merged over the profile.
//!
//! Maps merge key by key, everything else (sequences included) is replaced.
//! Two keys behave like in Clash Verge Rev: `dns` is merged one level deep,
//! so a sub-map such as `nameserver-policy` is replaced rather than mixed,
//! and `hosts` is replaced wholesale.

use crate::yaml::{Mapping, Value};

pub fn apply(config: &mut Mapping, overlay: &Mapping) {
    for (key, value) in overlay {
        let Some(name) = key.as_str() else { continue };
        let key = Value::from(name.to_ascii_lowercase());
        match (config.get_mut(&key), value) {
            (Some(Value::Mapping(dst)), Value::Mapping(src)) if name.eq_ignore_ascii_case("dns") => {
                for (k, v) in src {
                    dst.insert(k.clone(), v.clone());
                }
            }
            (Some(dst), _) if !name.eq_ignore_ascii_case("hosts") => deep_merge(dst, value.clone()),
            _ => {
                config.insert(key, value.clone());
            }
        }
    }
}

pub fn deep_merge(dst: &mut Value, src: Value) {
    match (dst, src) {
        (Value::Mapping(d), Value::Mapping(s)) => {
            for (k, v) in s {
                match d.get_mut(&k) {
                    Some(slot) => deep_merge(slot, v),
                    None => {
                        d.insert(k, v);
                    }
                }
            }
        }
        (dst, src) => *dst = src,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::parse_mapping;

    #[test]
    fn follows_clash_verge_semantics() {
        let mut config = parse_mapping(
            "rules: [a]\nTun: {mtu: 1500}\ndns:\n  nameserver: [9.9.9.9]\n  nameserver-policy: {old.example: 8.8.8.8}\nhosts: {old.example: 192.0.2.1}\n",
            "c",
        )
        .unwrap();
        let overlay = parse_mapping(
            "rules: [b]\ntun: {enable: true}\nDNS:\n  enable: true\n  nameserver-policy: {new.example: 1.1.1.1}\nhosts: {new.example: 192.0.2.2}\nextra: 1\n",
            "m",
        )
        .unwrap();
        apply(&mut config, &overlay);

        assert_eq!(config["rules"], serde_yaml_ng::from_str::<Value>("[b]").unwrap(), "sequences are replaced");
        assert_eq!(config["dns"]["nameserver"], serde_yaml_ng::from_str::<Value>("[9.9.9.9]").unwrap());
        assert!(config["dns"]["nameserver-policy"].get("old.example").is_none(), "dns sub-maps replaced");
        assert!(config["hosts"].get("old.example").is_none(), "hosts replaced");
        assert_eq!(config["extra"], Value::from(1));
        assert_eq!(config["tun"]["enable"], Value::from(true));
    }

    #[test]
    fn nested_maps_merge() {
        let mut config = parse_mapping("sniffer: {enable: true, sniff: {HTTP: {ports: [80]}}}", "c").unwrap();
        apply(&mut config, &parse_mapping("sniffer: {sniff: {TLS: {ports: [443]}}}", "m").unwrap());
        assert!(config["sniffer"]["sniff"].get("HTTP").is_some());
        assert!(config["sniffer"]["sniff"].get("TLS").is_some());
        assert_eq!(config["sniffer"]["enable"], Value::from(true));
    }
}
