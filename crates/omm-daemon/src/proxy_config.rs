//! Editor operations share Manager's lock with subscription refresh and rules.
use super::*;
use omm_config::{
    nodes::{self, Managed},
    yaml::{self, Mapping, Value as Yaml},
};
use serde_json::json;

#[derive(serde::Deserialize)]
struct Change {
    profile_id: String,
    revision: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    definition: Option<Mapping>,
    #[serde(default)]
    yaml: Option<String>,
    #[serde(default)]
    replacement: Option<String>,
    #[serde(default)]
    member_of: Option<Vec<String>>,
    #[serde(default)]
    items: Vec<Mapping>,
    #[serde(default)]
    replace: Vec<String>,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    group: bool,
    #[serde(default)]
    new_group: Option<String>,
}

struct View {
    profile: Profile,
    revision: String,
    config: Mapping,
    base: Mapping,
    managed: Managed,
    overrides: Overrides,
    global: Overrides,
    text: String,
    rules: Vec<omm_config::rules::UserRule>,
}

fn bad(message: impl Into<String>) -> RpcError {
    RpcError::new(ErrorCode::BadRequest, message)
}

fn view_dependencies(view: &View, target: &str) -> Vec<nodes::Dependency> {
    let mut dependencies = nodes::dependencies(&view.config, target);
    for rule in &view.rules {
        if view.managed.target(&rule.target) == target {
            dependencies.push(nodes::Dependency {
                owner: format!("user-rule:{}", rule.id),
                field: "rules".into(),
            });
        }
    }
    dependencies
}

impl Manager {
    fn proxy_view(&self, inner: &Inner) -> Result<View, RpcError> {
        let profile = inner
            .store
            .current()
            .cloned()
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "Select a profile first"))?;
        let text = inner.store.text(&profile.id).map_err(cfg_err)?;
        let global = Overrides::load(&self.paths, GLOBAL).map_err(cfg_err)?;
        let mut overrides = Overrides::load(&self.paths, &profile.id).map_err(cfg_err)?;
        let rules = inner.rules.rules().map_err(db_err)?;
        let build_with = |o: &Overrides| {
            build::inspect(&Inputs {
                profile: &text,
                profile_name: &profile.name,
                global: &global,
                overrides: o,
                user_rules: &rules,
                settings: &inner.settings,
            })
            .map_err(cfg_err)
        };
        let config = build_with(&overrides)?.config;
        let managed = std::mem::take(&mut overrides.nodes);
        let base = build_with(&overrides)?.config;
        overrides.nodes = managed.clone();
        let revision = omm_config::nodes::fingerprint(
            &serde_json::to_vec(&(
                &profile.id,
                &text,
                &global,
                &overrides,
                &rules,
                &inner.settings,
            ))
            .map_err(|e| bad(e.to_string()))?,
        );
        Ok(View {
            profile,
            revision,
            config,
            base,
            managed,
            overrides,
            global,
            text,
            rules,
        })
    }

    fn provider_nodes(&self, config: &Mapping) -> Vec<(String, Mapping)> {
        let mut result = Vec::new();
        if let Some(providers) = config.get("proxy-providers").and_then(Yaml::as_mapping) {
            for (name, p) in providers {
                let name = name.as_str().unwrap_or("");
                if let Some(payload) = p.get("payload").and_then(Yaml::as_sequence) {
                    result.extend(
                        payload
                            .iter()
                            .filter_map(Yaml::as_mapping)
                            .cloned()
                            .map(|m| (name.to_owned(), m)),
                    );
                } else if let Some(path) = p.get("path").and_then(Yaml::as_str) {
                    let path = self.paths.runtime().join(path);
                    if std::fs::metadata(&path).is_ok_and(|m| m.len() < 8 * 1024 * 1024)
                        && let Ok(text) = std::fs::read_to_string(path)
                        && let Ok(doc) = yaml::parse_mapping(&text, "provider")
                    {
                        result.extend(
                            nodes::definitions(&doc, false)
                                .into_iter()
                                .map(|m| (name.to_owned(), m)),
                        );
                    }
                }
            }
        }
        result
    }

    pub async fn proxy_read(
        &self,
        state: &State,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        if method == "proxy_catalog.get" {
            let mut catalog = nodes::catalog();
            let binary = self.binary.read().unwrap().clone();
            let version = match binary {
                Some(p) => omm_config::core_bin::binary_version(&p)
                    .await
                    .unwrap_or_default(),
                None => String::new(),
            };
            catalog["core_version"] = json!(version);
            catalog["supported"] = json!(nodes::supports_catalog(&version));
            return Ok(catalog);
        }
        let inner = self.inner.lock().await;
        let v = self.proxy_view(&inner)?;
        if let Some(id) = params.get("profile_id").and_then(Value::as_str)
            && id != v.profile.id
        {
            return Err(RpcError::new(
                ErrorCode::Conflict,
                "Profile changed; reopen the editor",
            ));
        }
        if method == "proxy_providers.get" {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let definition = v
                .config
                .get("proxy-providers")
                .and_then(Yaml::as_mapping)
                .and_then(|m| m.get(name))
                .ok_or_else(|| bad("Provider not found"))?;
            return Ok(
                json!({"profile_id":v.profile.id,"revision":v.revision,"definition":definition}),
            );
        }
        let group = method.starts_with("proxy_groups.");
        let get = method.ends_with(".get");
        let wanted = params.get("name").and_then(Value::as_str).unwrap_or("");
        let provider = params.get("provider").and_then(Value::as_str).unwrap_or("");
        let mut defs: Vec<(String, Mapping)> = nodes::definitions(&v.config, group)
            .into_iter()
            .map(|m| (String::new(), m))
            .collect();
        if !group {
            defs.extend(self.provider_nodes(&v.config));
        }
        let runtime = state.core.proxies().await.ok();
        let mut items = Vec::new();
        for (provider_name, m) in defs {
            let name = nodes::name(&m).to_owned();
            if get && (wanted != name || provider != provider_name) {
                continue;
            }
            let original = v.managed.original(group, &name);
            let edited = v.managed.entries(group).contains_key(&original);
            let inherited = nodes::definitions(&v.base, group)
                .iter()
                .any(|m| nodes::name(m) == original);
            let source = if !provider_name.is_empty() {
                "provider"
            } else if edited && inherited {
                "modified"
            } else if edited {
                "local"
            } else {
                "profile"
            };
            let px = runtime.as_ref().and_then(|r| r.proxies.get(&name));
            let mut item = json!({"name":name,"type":m.get("type"),"server":m.get("server"),"port":m.get("port"),"source":source,"provider":provider_name,"original":original,"resettable":edited,"dependencies":view_dependencies(&v,&name),"delay":px.and_then(|p|p.last_delay()),"now":px.and_then(|p|p.now.clone()),"members":m.get("proxies"),"runtime_members":px.and_then(|p|p.all.clone()),"automatic":m.get("include-all").and_then(Yaml::as_bool).unwrap_or(false) || m.get("include-all-proxies").and_then(Yaml::as_bool).unwrap_or(false) || m.get("include-all-providers").and_then(Yaml::as_bool).unwrap_or(false),"filter":m.get("filter")});
            if get {
                item["definition"] = serde_json::to_value(&m).map_err(|e| bad(e.to_string()))?;
                item["yaml"] = json!(yaml::to_string(&m).map_err(cfg_err)?);
            }
            items.push(item);
        }
        // Runtime-only provider nodes can be shown, but never fabricated into editable definitions.
        if !group
            && !get
            && let Some(runtime) = runtime.as_ref()
        {
            for (name, p) in &runtime.proxies {
                if !p.is_group()
                    && !nodes::BUILTINS.contains(&name.as_str())
                    && !items.iter().any(|i| i["name"] == *name)
                {
                    items.push(json!({"name":name,"type":p.kind,"source":"provider","provider":p.provider_name,"definition_available":false,"delay":p.last_delay(),"dependencies":[]}));
                }
            }
        }
        let providers: Vec<_> = v
            .config
            .get("proxy-providers")
            .and_then(Yaml::as_mapping)
            .into_iter()
            .flat_map(|m| m.keys())
            .filter_map(Yaml::as_str)
            .collect();
        let targets: Vec<_> = nodes::BUILTINS
            .iter()
            .filter(|n| **n != "GLOBAL")
            .map(|s| s.to_string())
            .chain(yaml::names(&v.config, "proxies").map(str::to_owned))
            .chain(yaml::names(&v.config, "proxy-groups").map(str::to_owned))
            .collect();
        let group_names: Vec<_> = yaml::names(&v.config, "proxy-groups").collect();
        let member_of: Vec<_> = nodes::definitions(&v.config, true)
            .iter()
            .filter(|m| {
                m.get("proxies")
                    .and_then(Yaml::as_sequence)
                    .is_some_and(|s| s.iter().any(|n| n.as_str() == Some(wanted)))
            })
            .map(|m| nodes::name(m).to_owned())
            .collect();
        // Deleted inherited entries remain discoverable for reset.
        let removed: Vec<_> = v
            .managed
            .entries(group)
            .iter()
            .filter(|(_, m)| m.is_none())
            .map(|(n, _)| n)
            .collect();
        if get && items.is_empty() {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                "Full definition unavailable; import the source URI or YAML",
            ));
        }
        Ok(
            json!({"profile_id":v.profile.id,"profile_name":v.profile.name,"revision":v.revision,"items":items,"targets":targets,"providers":providers,"groups":group_names,"member_of":member_of,"removed":removed}),
        )
    }

    pub async fn proxy_history(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let inner = self.inner.lock().await;
        let v = self.proxy_view(&inner)?;
        if params.get("profile_id").and_then(Value::as_str) != Some(&v.profile.id) {
            return Err(RpcError::new(ErrorCode::Conflict, "Profile changed"));
        }
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let group = method.starts_with("proxy_groups.");
        let original = v.managed.original(group, name);
        let path = self
            .paths
            .overrides(&v.profile.id)
            .join("editor-history.json");
        let mut items: Vec<Value> = omm_config::paths::read_optional(&path)
            .map_err(cfg_err)?
            .map(|s| serde_json::from_str::<Vec<Value>>(&s))
            .transpose()
            .map_err(|e| bad(e.to_string()))?
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r["name"] == original && r["group"] == group)
            .collect();
        items.reverse();
        if let Some(definition) = nodes::definitions(&v.base, group)
            .into_iter()
            .find(|m| nodes::name(m) == original)
        {
            items.push(json!({"original":true,"definition":definition}));
        }
        Ok(json!({"items":items}))
    }

    pub async fn proxy_change(
        &self,
        state: &State,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        let p: Change = serde_json::from_value(params).map_err(|e| bad(e.to_string()))?;
        let mut inner = self.inner.lock().await;
        let v = self.proxy_view(&inner)?;
        if p.profile_id != v.profile.id || p.revision != v.revision {
            return Err(RpcError::new(
                ErrorCode::Conflict,
                "Profile or subscription changed; reload before saving. Your input has not been applied.",
            ));
        }
        let preview = method == "proxy_config.preview";
        let operation = if preview {
            p.operation.as_str()
        } else {
            method.rsplit('.').next().unwrap_or("")
        };
        let group = if preview {
            p.group
        } else {
            method.starts_with("proxy_groups.")
        };
        let mut managed = v.managed.clone();
        let affected = view_dependencies(&v, &p.name);
        let mut definitions = if method == "proxy_import.commit" {
            p.items.clone()
        } else {
            Vec::new()
        };
        if operation == "save" && method != "proxy_providers.save" {
            let definition = if let Some(text) = &p.yaml {
                yaml::parse_mapping(text, "node editor").map_err(cfg_err)?
            } else {
                p.definition
                    .clone()
                    .ok_or_else(|| bad("definition or yaml is required"))?
            };
            definitions.push(definition);
        }
        if definitions.len() > 1000 {
            return Err(bad("Import limit is 1000 nodes"));
        }
        if method == "proxy_import.commit" && definitions.is_empty() {
            return Err(bad("Select at least one node"));
        }
        if preview && !["save", "remove", "reset"].contains(&operation) {
            return Err(bad("Invalid preview operation"));
        }
        let mut batch_names = std::collections::HashSet::new();
        for m in &definitions {
            if !batch_names.insert(nodes::name(m)) {
                return Err(bad("Duplicate names in import batch"));
            }
        }
        let mut existing = nodes::definitions(&v.config, false);
        existing.extend(nodes::definitions(&v.config, true));
        for m in &definitions {
            let name = nodes::name(m);
            if nodes::BUILTINS.contains(&name) || name.trim().is_empty() {
                return Err(bad("Choose a non-reserved name"));
            }
            let old = if method == "proxy_import.commit" {
                if p.replace.iter().any(|n| n == name) {
                    name
                } else {
                    ""
                }
            } else {
                &p.name
            };
            if old != name && existing.iter().any(|m| nodes::name(m) == name) {
                return Err(bad(format!("Name already exists: {name}")));
            }
            if !old.is_empty()
                && !nodes::definitions(&v.config, group)
                    .iter()
                    .any(|m| nodes::name(m) == old)
            {
                return Err(bad("Original node/group no longer exists"));
            }
            let key = if old.is_empty() {
                name.to_owned()
            } else {
                managed.original(group, old)
            };
            if old != name {
                // Reclaim the new name and flatten prior aliases. This also
                // permits renaming A → B → A without leaving a redirect cycle.
                let aliases: Vec<_> = managed
                    .redirects
                    .keys()
                    .filter(|alias| managed.target(alias) == old)
                    .cloned()
                    .collect();
                managed.redirects.remove(name);
                for alias in aliases
                    .into_iter()
                    .chain((!old.is_empty()).then(|| old.to_owned()))
                {
                    if alias != name {
                        managed.redirects.insert(alias, name.to_owned());
                    }
                }
            }
            managed.entries_mut(group).insert(key, Some(m.clone()));
            existing.push(m.clone());
        }
        if operation == "remove" || operation == "reset" {
            if nodes::BUILTINS.contains(&p.name.as_str()) {
                return Err(bad("Built-in policies cannot be edited"));
            }
            let key = managed.original(group, &p.name);
            if operation == "reset" {
                let current = managed
                    .entries_mut(group)
                    .remove(&key)
                    .ok_or_else(|| bad("No local override"))?;
                managed.redirects.remove(&key);
                if let Some(m) = current {
                    let name = nodes::name(&m);
                    if name != key {
                        managed.redirects.insert(name.into(), key.clone());
                    }
                }
            } else {
                if !nodes::definitions(&v.config, group)
                    .iter()
                    .any(|m| nodes::name(m) == p.name)
                {
                    return Err(bad("Node/group not found"));
                }
                if let Some(replacement) = &p.replacement {
                    if replacement == &p.name
                        || !existing.iter().any(|m| nodes::name(m) == replacement)
                            && !nodes::BUILTINS.contains(&replacement.as_str())
                    {
                        return Err(bad("Invalid replacement"));
                    }
                    managed
                        .redirects
                        .insert(p.name.clone(), replacement.clone());
                } else {
                    let mandatory = affected.iter().any(|d| d.field != "proxies");
                    if mandatory {
                        return Err(bad(
                            "Choose a replacement for dependent rules, defaults or dialer chains",
                        ));
                    }
                    for mut m in nodes::definitions(&v.config, true) {
                        if let Some(Yaml::Sequence(members)) = m.get_mut("proxies") {
                            let before = members.len();
                            members.retain(|n| n.as_str() != Some(&p.name));
                            if before != members.len() {
                                let k = managed.original(true, nodes::name(&m));
                                managed.groups.insert(k, Some(m));
                            }
                        }
                    }
                }
                managed.entries_mut(group).insert(key, None);
            }
        }
        if method == "proxy_providers.save" {
            if !v
                .config
                .get("proxy-providers")
                .and_then(Yaml::as_mapping)
                .is_some_and(|m| m.contains_key(&p.name))
            {
                return Err(bad("Provider not found"));
            }
            managed.providers.insert(
                p.name.clone(),
                p.definition
                    .clone()
                    .ok_or_else(|| bad("Provider definition required"))?,
            );
        }
        // Explicit member selection; no automatic insertion into a selector.
        if !group && let Some(selected) = &p.member_of {
            for name in selected {
                if !nodes::definitions(&v.config, true)
                    .iter()
                    .any(|m| nodes::name(m) == name)
                {
                    return Err(bad(format!("Unknown group: {name}")));
                }
            }
            for def in &definitions {
                let new = nodes::name(def);
                for mut m in nodes::definitions(&v.config, true) {
                    let key = managed.original(true, nodes::name(&m));
                    if let Some(Some(edited)) = managed.groups.get(&key) {
                        m = edited.clone();
                    }
                    let before = m.clone();
                    let selected = selected.iter().any(|n| n == nodes::name(&m));
                    if selected || m.contains_key("proxies") {
                        let members = m
                            .entry("proxies".into())
                            .or_insert_with(|| Yaml::Sequence(Vec::new()));
                        if let Some(a) = members.as_sequence_mut() {
                            let mut found = false;
                            a.retain_mut(|n| {
                                if n.as_str() == Some(new)
                                    || (!p.name.is_empty() && n.as_str() == Some(&p.name))
                                {
                                    if !selected || found {
                                        return false;
                                    }
                                    *n = new.into();
                                    found = true;
                                }
                                true
                            });
                            if selected && !found {
                                a.push(new.into());
                            }
                        }
                    }
                    if m != before {
                        managed.groups.insert(key, Some(m));
                    }
                }
            }
        }
        if let Some(name) = &p.new_group {
            if method != "proxy_import.commit"
                || name.trim() != name
                || name.is_empty()
                || nodes::BUILTINS.contains(&name.as_str())
                || existing.iter().any(|m| nodes::name(m) == name)
                || definitions.iter().any(|m| nodes::name(m) == name)
            {
                return Err(bad("Choose a unique name for the imported group"));
            }
            let definition = serde_json::from_value::<Mapping>(json!({
                "name": name, "type":"select", "proxies":definitions.iter().map(nodes::name).collect::<Vec<_>>()
            })).map_err(|e| bad(e.to_string()))?;
            managed.groups.insert(name.clone(), Some(definition));
        }
        // Reject cycles in replacement chains or manually edited overlays.
        for start in managed.redirects.keys() {
            let mut seen = std::collections::HashSet::new();
            let mut n = start;
            while let Some(next) = managed.redirects.get(n) {
                if !seen.insert(n) {
                    return Err(bad(
                        "Rename/replacement would create a redirect cycle; restore the original first",
                    ));
                }
                n = next;
            }
        }
        let mut overrides = v.overrides.clone();
        overrides.nodes = managed.clone();
        let out = build::build(&Inputs {
            profile: &v.text,
            profile_name: &v.profile.name,
            global: &v.global,
            overrides: &overrides,
            user_rules: &v.rules,
            settings: &inner.settings,
        })
        .map_err(cfg_err)?;
        nodes::validate(&out.config).map_err(cfg_err)?;
        let previously_orphaned = build::inspect(&Inputs {
            profile: &v.text,
            profile_name: &v.profile.name,
            global: &v.global,
            overrides: &v.overrides,
            user_rules: &v.rules,
            settings: &inner.settings,
        })
        .map_err(cfg_err)?
        .orphans;
        if out
            .orphans
            .iter()
            .any(|id| !previously_orphaned.contains(id))
        {
            return Err(bad(
                "This change would orphan a user rule; choose a replacement",
            ));
        }
        let yaml = out.yaml().map_err(cfg_err)?;
        let runtime = self.runtime()?;
        runtime.validate(&yaml).await.map_err(|_|RpcError::new(ErrorCode::Config,"Mihomo rejected this configuration. Check the protocol fields and referenced targets; nothing was applied."))?;
        if preview {
            return Ok(
                json!({"valid":true,"definition":definitions.first(),"yaml":definitions.first().map(yaml::to_string).transpose().map_err(cfg_err)?,"dependencies":affected,"targets":out.targets}),
            );
        }
        let previous_live =
            omm_config::paths::read_optional(&self.paths.runtime_config()).map_err(cfg_err)?;
        let previous_status = inner.status.clone();
        let previous_targets = inner.targets.clone();
        // Capture the previous definition only for successful editor transactions.
        let history_path = self
            .paths
            .overrides(&v.profile.id)
            .join("editor-history.json");
        let mut history: Vec<Value> = omm_config::paths::read_optional(&history_path)
            .map_err(cfg_err)?
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| bad(e.to_string()))?
            .unwrap_or_default();
        if let Some(previous) = nodes::definitions(&v.config, group)
            .into_iter()
            .find(|m| nodes::name(m) == p.name)
        {
            let member_of: Vec<_> = nodes::definitions(&v.config, true)
                .into_iter()
                .filter(|m| {
                    m.get("proxies")
                        .and_then(Yaml::as_sequence)
                        .is_some_and(|a| a.iter().any(|n| n.as_str() == Some(&p.name)))
                })
                .map(|m| nodes::name(&m).to_owned())
                .collect();
            history.push(json!({"name":v.managed.original(group,&p.name),"group":group,"at":profiles::now(),"definition":previous,"member_of":member_of}));
            if history.len() > 100 {
                history.drain(..history.len() - 100);
            }
        }
        managed.save(&self.paths, &v.profile.id).map_err(cfg_err)?;
        let install = runtime.install(&yaml).map_err(cfg_err);
        let result = match install {
            Ok(live) => {
                if self.owns_core(state) && state.core_running() {
                    state
                        .core
                        .reload_config(&live, false)
                        .await
                        .map(|_| true)
                        .map_err(|_| RpcError::new(ErrorCode::Core, "Mihomo reload failed"))
                } else {
                    Ok(false)
                }
            }
            Err(e) => Err(e),
        };
        match result {
            Ok(reloaded) => {
                inner.status = ConfigStatus {
                    profile: Some(ProfileRef {
                        id: v.profile.id.clone(),
                        name: v.profile.name.clone(),
                    }),
                    applied_at: Some(profiles::now()),
                    orphans: out.orphans,
                    error: None,
                };
                inner.targets = out.targets;
                state.set_config_status(inner.status.clone());
                state.request_refresh();
                let revision = self.proxy_view(&inner)?.revision;
                // History is ancillary: do not report an applied change as a failure if its archive cannot be written.
                let history_saved = serde_json::to_vec(&history).ok().is_some_and(|data| {
                    omm_config::paths::write_atomic(&history_path, &data).is_ok()
                });
                Ok(
                    json!({"ok":true,"reloaded":reloaded,"profile_id":v.profile.id,"revision":revision,"history_saved":history_saved}),
                )
            }
            Err(error) => {
                let mut failures = Vec::new();
                if v.managed.save(&self.paths, &v.profile.id).is_err() {
                    failures.push("overrides");
                }
                if let Some(previous) = previous_live {
                    if omm_config::paths::write_atomic(
                        &self.paths.runtime_config(),
                        previous.as_bytes(),
                    )
                    .is_err()
                    {
                        failures.push("runtime file");
                    } else if self.owns_core(state)
                        && state.core_running()
                        && state
                            .core
                            .reload_config(&self.paths.runtime_config(), false)
                            .await
                            .is_err()
                    {
                        failures.push("core");
                    }
                }
                inner.status = previous_status;
                inner.targets = previous_targets;
                let message = if failures.is_empty() {
                    format!("{}; previous configuration restored", error.message)
                } else {
                    format!(
                        "{}; restoration failed: {}",
                        error.message,
                        failures.join(", ")
                    )
                };
                inner.status.error = Some(message.clone());
                state.set_config_status(inner.status.clone());
                Err(RpcError::new(ErrorCode::Core, message))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROFILE: &str = "proxies: [{name: A, type: direct}, {name: B, type: direct}]\nproxy-groups: [{name: G, type: select, proxies: [A, B, DIRECT]}]\nrules: ['DOMAIN,example.net,A','MATCH,G']\n";
    async fn setup() -> Option<(tempfile::TempDir, State)> {
        let binary = std::env::var_os("OMM_TEST_MIHOMO").map(PathBuf::from)?;
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let manager = Manager::open(paths.clone(), Some(binary)).unwrap();
        let proxy =
            crate::system_proxy::Manager::open(dir.path().join("proxy-state.json")).unwrap();
        let state = State::new(
            mihomo_api::Client::new(paths.controller_socket()),
            manager,
            omm_problems::Collector::open(&dir.path().join("problems.db")).unwrap(),
            proxy,
        );
        state
            .config
            .add_local(&state, "test".into(), PROFILE.into())
            .await
            .unwrap();
        Some((dir, state))
    }
    async fn read(state: &State) -> Value {
        state
            .config
            .proxy_read(state, "proxy_nodes.list", Value::Null)
            .await
            .unwrap()
    }
    async fn change(state: &State, method: &str, mut p: Value) -> Result<Value, RpcError> {
        let v = read(state).await;
        p["profile_id"] = v["profile_id"].clone();
        p["revision"] = v["revision"].clone();
        state.config.proxy_change(state, method, p).await
    }
    #[tokio::test]
    async fn provider_edit_preserves_payload_and_revision_guard() {
        let Some((_dir, s)) = setup().await else {
            return;
        };
        let before = read(&s).await;
        {
            let inner = s.config.inner.lock().await;
            inner.store.write_text(before["profile_id"].as_str().unwrap(), &format!("{PROFILE}proxy-providers:\n  source:\n    type: inline\n    payload: [{{name: P, type: direct}}]\n")).unwrap();
        }
        let provider = s
            .config
            .proxy_read(
                &s,
                "proxy_providers.get",
                json!({"profile_id":before["profile_id"],"name":"source"}),
            )
            .await
            .unwrap();
        let mut definition = provider["definition"].clone();
        definition["filter"] = json!("P");
        change(
            &s,
            "proxy_providers.save",
            json!({"name":"source","definition":definition}),
        )
        .await
        .unwrap();
        let after = s
            .config
            .proxy_read(&s, "proxy_providers.get", json!({"name":"source"}))
            .await
            .unwrap();
        assert_eq!(after["definition"]["filter"], "P");
        assert_eq!(after["definition"]["payload"][0]["name"], "P");
        assert_ne!(after["revision"], provider["revision"]);
        let stale = s.config.proxy_change(&s,"proxy_providers.save",json!({"profile_id":provider["profile_id"],"revision":provider["revision"],"name":"source","definition":definition})).await.unwrap_err();
        assert_eq!(stale.code, ErrorCode::Conflict);
    }
    #[tokio::test]
    async fn editor_history_and_import_group_are_revision_guarded() {
        let Some((_dir, s)) = setup().await else {
            return;
        };
        let before = read(&s).await;
        let saved = change(
            &s,
            "proxy_nodes.save",
            json!({"name":"A","definition":{"name":"A","type":"direct","udp":false}}),
        )
        .await
        .unwrap();
        assert_ne!(saved["revision"], before["revision"]);
        let history = s
            .config
            .proxy_history(
                "proxy_nodes.history",
                json!({"profile_id":before["profile_id"],"name":"A"}),
            )
            .await
            .unwrap();
        assert!(history["items"].as_array().unwrap().len() >= 2);
        assert_eq!(history["items"][0]["definition"]["name"], "A");
        assert!(history["items"][0]["definition"].get("udp").is_none());
        assert_eq!(history["items"][0]["member_of"], json!(["G"]));
        let rejected = change(
            &s,
            "proxy_import.commit",
            json!({"items":[{"name":"New","type":"direct"}],"new_group":"G"}),
        )
        .await;
        assert!(rejected.is_err());
        assert!(
            !read(&s).await["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["name"] == "New")
        );
        change(
            &s,
            "proxy_import.commit",
            json!({"items":[{"name":"New","type":"direct"}],"new_group":"Imported"}),
        )
        .await
        .unwrap();
        let groups = s
            .config
            .proxy_read(&s, "proxy_groups.get", json!({"name":"Imported"}))
            .await
            .unwrap();
        assert_eq!(groups["items"][0]["definition"]["proxies"], json!(["New"]));
    }
    #[tokio::test]
    async fn edit_rename_refresh_reset_and_conflict() {
        let Some((_dir, s)) = setup().await else {
            return;
        };
        let old = read(&s).await;
        change(&s,"proxy_nodes.save",json!({"name":"A","definition":{"name":"A2","type":"direct","future-option":{"keep":true}},"member_of":["G"]})).await.unwrap();
        let inner = s.config.inner.lock().await;
        let v = s.config.proxy_view(&inner).unwrap();
        assert_eq!(
            v.config["proxy-groups"][0]["proxies"][0].as_str(),
            Some("A2"),
            "editing must retain fallback/selector order"
        );
        assert_eq!(v.config["rules"][0].as_str(), Some("DOMAIN,example.net,A2"));
        let profile_id = v.profile.id.clone();
        drop(inner);
        let stale=s.config.proxy_change(&s,"proxy_nodes.save",json!({"profile_id":old["profile_id"],"revision":old["revision"],"name":"B","definition":{"name":"B","type":"direct"}})).await.unwrap_err();
        assert_eq!(stale.code, ErrorCode::Conflict);
        {
            let inner = s.config.inner.lock().await;
            inner
                .store
                .write_text(
                    &profile_id,
                    &PROFILE.replace(
                        "type: direct}, {name: B",
                        "type: direct, udp: false}, {name: B",
                    ),
                )
                .unwrap();
        }
        let v = read(&s).await;
        assert!(
            v["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["name"] == "A2")
        );
        change(&s, "proxy_nodes.reset", json!({"name":"A2"}))
            .await
            .unwrap();
        let inner = s.config.inner.lock().await;
        let v = s.config.proxy_view(&inner).unwrap();
        assert_eq!(v.config["proxies"][0]["name"].as_str(), Some("A"));
        assert_eq!(v.config["proxies"][0]["udp"].as_bool(), Some(false));
        drop(inner);
        change(
            &s,
            "proxy_nodes.save",
            json!({"name":"A","definition":{"name":"A2","type":"direct"}}),
        )
        .await
        .unwrap();
        change(
            &s,
            "proxy_nodes.save",
            json!({"name":"A2","definition":{"name":"A","type":"direct","udp":true}}),
        )
        .await
        .unwrap();
        let inner = s.config.inner.lock().await;
        let v = s.config.proxy_view(&inner).unwrap();
        assert_eq!(
            v.config["proxy-groups"][0]["proxies"][0].as_str(),
            Some("A")
        );
        assert_eq!(v.config["proxies"][0]["udp"].as_bool(), Some(true));
    }
    #[tokio::test]
    async fn rule_redirects_are_profile_local_and_disabled_rules_keep_dependencies() {
        let Some((_dir, s)) = setup().await else {
            return;
        };
        let (stored, _) = s
            .config
            .change_rules(&s, true, |store| {
                let (stored, change) = store
                    .upsert(
                        crate::user_rules::NewRule {
                            kind: omm_config::rules::MatchKind::Domain,
                            value: "user.example.net".into(),
                            target: "B".into(),
                            note: None,
                            source: "test".into(),
                            test: None,
                            lifetime: Default::default(),
                        },
                        1,
                    )
                    .map_err(db_err)?;
                Ok((stored, vec![change]))
            })
            .await
            .unwrap();
        let original = read(&s).await["profile_id"].as_str().unwrap().to_owned();
        change(
            &s,
            "proxy_nodes.save",
            json!({"name":"B","definition":{"name":"B2","type":"direct"}}),
        )
        .await
        .unwrap();
        for enabled in [false, true, false] {
            s.config
                .change_rules(&s, true, |store| {
                    let (_, change) = store
                        .update(
                            stored.rule.id,
                            crate::user_rules::Patch {
                                enabled: Some(enabled),
                                ..Default::default()
                            },
                            2,
                        )
                        .map_err(db_err)?
                        .unwrap();
                    Ok(((), vec![change]))
                })
                .await
                .unwrap();
        }
        assert_eq!(s.config.rules().await.unwrap().0[0].rule.target, "B");
        let error = change(&s, "proxy_nodes.remove", json!({"name":"B2"}))
            .await
            .unwrap_err();
        assert!(error.message.contains("replacement"));
        let second = s
            .config
            .add_local(&s, "second".into(), PROFILE.into())
            .await
            .unwrap();
        s.config
            .select(&s, second["id"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(s.config.rule_redirects().await.unwrap().target("B"), "B");
        assert!(
            read(&s).await["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["name"] == "B")
        );
        s.config.select(&s, &original).await.unwrap();
        assert_eq!(s.config.rule_redirects().await.unwrap().target("B"), "B2");
    }
    #[tokio::test]
    async fn deletion_requires_replacement_and_batch_is_atomic() {
        let Some((_dir, s)) = setup().await else {
            return;
        };
        assert!(
            change(&s, "proxy_nodes.remove", json!({"name":"A"}))
                .await
                .is_err()
        );
        change(
            &s,
            "proxy_nodes.remove",
            json!({"name":"A","replacement":"B"}),
        )
        .await
        .unwrap();
        let before = std::fs::read(s.config.paths.runtime_config()).unwrap();
        let invalid=change(&s,"proxy_import.commit",json!({"items":[{"name":"New","type":"direct"},{"name":"Broken","type":"unsupported-test","server":"example.net","port":443,"uuid":"not-a-uuid"}],"member_of":["G"]})).await;
        assert!(invalid.is_err());
        assert_eq!(
            std::fs::read(s.config.paths.runtime_config()).unwrap(),
            before
        );
        assert!(
            !read(&s).await["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["name"] == "New")
        );
        change(
            &s,
            "proxy_import.commit",
            json!({"items":[{"name":"New","type":"direct"}],"member_of":["G"]}),
        )
        .await
        .unwrap();
        assert!(
            read(&s).await["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["name"] == "New")
        );
    }
    #[tokio::test]
    async fn reload_failure_restores_files_and_reports_recovery_failure() {
        let Some((_dir, s)) = setup().await else {
            return;
        };
        let before = std::fs::read(s.config.paths.runtime_config()).unwrap();
        s.set_core_status(omm_proto::CoreStatus {
            running: true,
            ..Default::default()
        });
        let err = change(
            &s,
            "proxy_nodes.save",
            json!({"name":"B","definition":{"name":"B","type":"direct","udp":false}}),
        )
        .await
        .unwrap_err();
        assert!(err.message.contains("restoration failed: core"), "{err}");
        assert_eq!(
            std::fs::read(s.config.paths.runtime_config()).unwrap(),
            before
        );
        assert!(
            read(&s).await["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["source"] == "profile")
        );
    }
    #[tokio::test]
    async fn applies_to_a_live_isolated_core() {
        let Some((_dir, s)) = setup().await else {
            return;
        };
        {
            let mut inner = s.config.inner.lock().await;
            inner.settings.tun.enable = false;
            inner.settings.mixed_port = 0;
        }
        s.config.apply(&s).await.unwrap();
        let binary = s.config.runtime().unwrap().binary;
        let mut child = tokio::process::Command::new(binary)
            .arg("-d")
            .arg(s.config.paths.runtime())
            .arg("-f")
            .arg(s.config.paths.runtime_config())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        for _ in 0..100 {
            if s.core.version().await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        s.core.version().await.unwrap();
        s.set_core_status(omm_proto::CoreStatus {
            running: true,
            ..Default::default()
        });
        let result = change(
            &s,
            "proxy_nodes.save",
            json!({"definition":{"name":"Live","type":"direct"},"member_of":["G"]}),
        )
        .await
        .unwrap();
        assert_eq!(result["reloaded"], true);
        let proxies = s.core.proxies().await.unwrap();
        assert!(proxies.proxies.contains_key("Live"));
        assert!(
            proxies.proxies["G"]
                .all
                .as_ref()
                .unwrap()
                .iter()
                .any(|n| n == "Live")
        );
        child.kill().await.unwrap();
        child.wait().await.unwrap();
    }
}
