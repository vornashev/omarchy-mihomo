//! `omm`: thin client for `omarchy-mihomod`.

mod fix;
mod system;

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use omm_proto::{ProxyGroup, Request, ServerFrame, Status};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

#[derive(Debug, Parser)]
#[command(name = "omm", version, about = "Control omarchy-mihomo")]
struct Cli {
    /// Daemon socket.
    #[arg(long, global = true, env = "OMM_SOCKET", default_value_os_t = omm_proto::default_socket_path())]
    socket: PathBuf,

    /// Print raw JSON instead of a summary.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Daemon and core state (default).
    Status,
    /// List proxy groups, or the members of one group.
    Proxies { group: Option<String> },
    /// Select a member of a selector group.
    Select { group: String, name: String },
    /// Measure latency of a proxy or group.
    Delay {
        name: String,
        #[arg(long)]
        url: Option<String>,
        /// Test every member of the group.
        #[arg(long)]
        group: bool,
    },
    /// Switch routing mode.
    Mode { mode: ModeArg },
    /// Toggle TUN.
    Tun { state: OnOff },
    /// List active connections.
    Conns,
    /// Manage profiles (subscriptions and local configs).
    #[command(subcommand)]
    Profile(ProfileCmd),
    /// Failed connections, merged per resource.
    #[command(subcommand)]
    Problems(ProblemsCmd),
    /// Your own routing rules (above the profile's).
    #[command(subcommand)]
    Rules(RulesCmd),
    /// Rebuild the runtime config and hot-reload the core.
    Apply,
    /// Restore a previous runtime config (the latest by default).
    Rollback { name: Option<String> },
    /// List saved runtime configs.
    Snapshots,
    /// Show settings, or change them: `omm settings '{"tun":{"stack":"system"}}'`.
    Settings { patch: Option<String> },
    /// Stream events (status, traffic) as JSON lines.
    Watch { topics: Vec<String> },
    /// Install or control the mihomo core.
    #[command(subcommand)]
    Core(CoreCmd),
    /// Check the installation and say how to fix what is wrong.
    Doctor,
    /// List setup issues, or run the fix with this id (some ask for sudo).
    Fix { id: Option<String> },
    /// Move over from another proxy client.
    #[command(subcommand)]
    Migrate(MigrateCmd),
    /// Call any daemon method: `omm call proxies.delay '{"name":"AI"}'`.
    Call { method: String, params: Option<String> },
}

#[derive(Debug, Subcommand)]
enum MigrateCmd {
    /// Import Clash Verge Rev's profiles, switch it off (service, app,
    /// autostart; nothing is deleted) and start our core. Asks for sudo.
    Cvr {
        /// Switch Clash Verge back on and stop our core.
        #[arg(long)]
        undo: bool,
    },
}

#[derive(Debug, Subcommand)]
enum CoreCmd {
    /// Download, verify (sha256) and install the core binary.
    Install {
        /// Release tag, or `latest`. Default: the pinned, tested version.
        #[arg(long)]
        version: Option<String>,
        /// Download through a proxy, e.g. http://127.0.0.1:7897.
        #[arg(long)]
        proxy: Option<String>,
    },
    /// Give the core the capabilities TUN needs (asks for sudo).
    Setcap,
    Start,
    Stop,
    Restart,
}

#[derive(Debug, Subcommand)]
enum ProfileCmd {
    /// List profiles (default).
    List,
    /// Add a subscription URL or a local file.
    Add {
        /// `https://...` subscription or path to a YAML file.
        source: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        user_agent: Option<String>,
    },
    /// Create an empty local profile and open it in $VISUAL/$EDITOR;
    /// paste a config and save, the daemon takes it in.
    New {
        #[arg(long)]
        name: Option<String>,
    },
    /// Open a local profile's file in $VISUAL/$EDITOR.
    Edit { profile: String },
    /// Refresh one profile, or all remote ones.
    Update {
        profile: Option<String>,
        /// Ignore ETag and refresh intervals.
        #[arg(long)]
        force: bool,
    },
    /// Make a profile current.
    Use { profile: String },
    /// Delete a profile and its overrides.
    Rm { profile: String },
}

#[derive(Debug, Subcommand)]
enum ProblemsCmd {
    /// List problems, newest first.
    List {
        /// Only those since the last `omm problems seen`.
        #[arg(long)]
        new: bool,
        /// Show hidden problems instead.
        #[arg(long)]
        hidden: bool,
        /// timeout, reset, refused, dns_fail, dns_stub, no_route, tls, proxy, other
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        app: Option<String>,
        /// Substring of host or app.
        search: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Mark everything as seen (resets the bar badge).
    Seen,
    /// Hide a host, a domain with subdomains (--suffix) or an app (--app).
    Hide {
        value: String,
        #[arg(long, conflicts_with = "app")]
        suffix: bool,
        #[arg(long)]
        app: bool,
    },
    /// Show what is hidden.
    Ignores,
    /// Stop hiding (id from `omm problems ignores`).
    Unhide { id: i64 },
    /// Forget one host, or everything.
    Clear { host: Option<String> },
}

#[derive(Debug, Subcommand)]
enum RulesCmd {
    /// List rules (default order is the order they apply in).
    List,
    /// Route a domain, URL or IP: `omm rules add claude.ai --via AI`.
    Add {
        value: String,
        /// DIRECT or a group of the current profile.
        #[arg(long)]
        via: String,
        /// Only this exact host, not its subdomains.
        #[arg(long)]
        exact: bool,
        /// Check the route first and refuse to save if it fails.
        #[arg(long)]
        check: bool,
        #[arg(long)]
        note: Option<String>,
    },
    /// Check how a host behaves through some targets.
    Test {
        host: String,
        #[arg(required = true)]
        targets: Vec<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    /// Change a rule's target or switch it on/off.
    Set {
        id: i64,
        #[arg(long)]
        via: Option<String>,
        #[arg(long, conflicts_with = "off")]
        on: bool,
        #[arg(long)]
        off: bool,
    },
    Rm { id: i64 },
    /// Journal of rule changes.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Undo every change after journal entry <to> (0 = undo all).
    Rollback { to: i64 },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ModeArg {
    Rule,
    Global,
    Direct,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

fn main() {
    // Die quietly on a closed pipe (`omm conns | head`) like other unix tools.
    // SAFETY: called before any other thread exists.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("omm: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    // These work without the daemon.
    match &cli.cmd {
        Some(Cmd::Core(c)) => {
            return match c {
                CoreCmd::Install { version, proxy } => system::core_install(version.as_deref(), proxy.as_deref()),
                CoreCmd::Setcap => system::core_setcap(),
                CoreCmd::Start => system::core_unit("start"),
                CoreCmd::Stop => system::core_unit("stop"),
                CoreCmd::Restart => system::core_unit("restart"),
            };
        }
        Some(Cmd::Fix { id }) => return fix::run(&cli.socket, id.as_deref()),
        Some(Cmd::Migrate(MigrateCmd::Cvr { undo })) => {
            return if *undo { fix::undo_cvr(&cli.socket) } else { fix::migrate_cvr(&cli.socket) };
        }
        Some(Cmd::Doctor) => {
            let status = Conn::open(&cli.socket).ok().and_then(|mut c| c.call::<Value>("status", Value::Null).ok());
            if !system::doctor(status)? {
                std::process::exit(1);
            }
            return Ok(());
        }
        _ => {}
    }
    let mut conn = Conn::open(&cli.socket)?;
    let json = cli.json;
    match cli.cmd.unwrap_or(Cmd::Status) {
        Cmd::Status => {
            let s: Status = conn.call("status", Value::Null)?;
            if json {
                return print_json(&s);
            }
            let c = &s.core;
            println!("daemon  {} (protocol {})", s.daemon_version, s.protocol);
            if c.running {
                println!(
                    "core    running · {} · mode {} · tun {} · mixed-port {}",
                    c.version.as_deref().unwrap_or("?"),
                    c.mode.as_deref().unwrap_or("?"),
                    if c.tun { "on" } else { "off" },
                    c.mixed_port
                );
                if let Some(t) = &c.tun_issue {
                    println!("tun     wanted but off: {}", tun_reasons(t).join("; "));
                }
            } else {
                println!("core    not running ({})", c.error.as_deref().unwrap_or(&c.socket));
            }
        }
        Cmd::Proxies { group } => {
            let groups: Vec<ProxyGroup> = conn.call("proxies.list", Value::Null)?;
            let groups: Vec<_> = match &group {
                Some(g) => groups.into_iter().filter(|x| &x.name == g).collect(),
                None => groups,
            };
            if group.is_some() && groups.is_empty() {
                bail!("no such group");
            }
            if json {
                return print_json(&groups);
            }
            for g in &groups {
                println!("{:<24} {:<11} → {}", g.name, g.kind, g.now.as_deref().unwrap_or("-"));
                if group.is_some() {
                    for m in &g.members {
                        let mark = if g.now.as_deref() == Some(&m.name) { "*" } else { " " };
                        println!("  {mark} {:<28} {:<10} {}", m.name, m.kind, fmt_delay(m.delay));
                    }
                }
            }
        }
        Cmd::Select { group, name } => {
            conn.call::<Value>("proxies.select", json!({ "group": group, "name": name }))?;
        }
        Cmd::Delay { name, url, group } => {
            let method = if group { "group.delay" } else { "proxies.delay" };
            let r: Value = conn.call(method, json!({ "name": name, "url": url }))?;
            if json || group {
                return print_json(&r);
            }
            match r.get("error").and_then(Value::as_str) {
                Some(err) => println!("{name}: failed ({err})"),
                None => println!("{name}: {}", fmt_delay(r["delay"].as_u64().map(|d| d as u32))),
            }
        }
        Cmd::Mode { mode } => {
            let mode = format!("{mode:?}").to_lowercase();
            conn.call::<Value>("mode.set", json!({ "mode": mode }))?;
        }
        Cmd::Tun { state } => {
            conn.call::<Value>("tun.set", json!({ "enable": matches!(state, OnOff::On) }))?;
        }
        Cmd::Conns => {
            let r: Value = conn.call("connections.list", Value::Null)?;
            if json {
                return print_json(&r);
            }
            for c in r["connections"].as_array().into_iter().flatten() {
                let m = &c["metadata"];
                let host = [&m["host"], &m["sniffHost"], &m["destinationIP"]]
                    .into_iter()
                    .filter_map(Value::as_str)
                    .find(|s| !s.is_empty())
                    .unwrap_or("?");
                let chain: Vec<_> = c["chains"].as_array().into_iter().flatten().rev().filter_map(Value::as_str).collect();
                println!(
                    "{:<40} {:<18} {}",
                    format!("{host}:{}", m["destinationPort"].as_str().unwrap_or("")),
                    m["process"].as_str().filter(|s| !s.is_empty()).unwrap_or("-"),
                    chain.join(" → ")
                );
            }
        }
        Cmd::Profile(cmd) => profile(&mut conn, cmd, json)?,
        Cmd::Problems(cmd) => problems(&mut conn, cmd, json)?,
        Cmd::Rules(cmd) => rules(&mut conn, cmd, json)?,
        Cmd::Apply => {
            let r: Value = conn.call("config.apply", Value::Null)?;
            if json {
                return print_json(&r);
            }
            println!(
                "applied {} · {} groups · {}",
                r["profile"]["name"].as_str().unwrap_or("?"),
                r["groups"].as_array().map_or(0, Vec::len),
                if r["reloaded"] == true { "core reloaded" } else { "core not reloaded (not running or not ours)" }
            );
            if let Some(o) = r["orphans"].as_array().filter(|o| !o.is_empty()) {
                println!("skipped {} rule(s) whose group is gone", o.len());
            }
        }
        Cmd::Rollback { name } => {
            let r: Value = conn.call("config.rollback", json!({ "name": name }))?;
            print_json(&r)?;
        }
        Cmd::Snapshots => {
            let r: Value = conn.call("config.snapshots", Value::Null)?;
            if json {
                return print_json(&r);
            }
            for s in r.as_array().into_iter().flatten() {
                println!("{}  {}", s["name"].as_str().unwrap_or(""), ago(s["time"].as_u64()));
            }
        }
        Cmd::Settings { patch } => {
            let r: Value = match patch {
                Some(p) => conn.call("settings.set", serde_json::from_str(&p).context("settings patch must be JSON")?)?,
                None => conn.call("settings.get", Value::Null)?,
            };
            print_json(&r)?;
        }
        Cmd::Watch { topics } => {
            let params = if topics.is_empty() { Value::Null } else { json!({ "topics": topics }) };
            conn.call::<Value>("events.subscribe", params)?;
            let stdout = std::io::stdout();
            loop {
                let line = match conn.pending.pop_front() {
                    Some(l) => l,
                    None => conn.read_line()?,
                };
                let mut out = stdout.lock();
                writeln!(out, "{line}")?;
                out.flush()?;
            }
        }
        Cmd::Core(_) | Cmd::Doctor | Cmd::Fix { .. } | Cmd::Migrate(_) => unreachable!("handled above"),
        Cmd::Call { method, params } => {
            let params = match params {
                Some(p) => serde_json::from_str(&p).context("params must be JSON")?,
                None => Value::Null,
            };
            let r: Value = conn.call(&method, params)?;
            print_json(&r)?;
        }
    }
    Ok(())
}

fn profile(conn: &mut Conn, cmd: ProfileCmd, json: bool) -> Result<()> {
    // Profiles can be named by id or by (unique) name.
    let resolve = |conn: &mut Conn, key: &str| -> Result<String> {
        let list: Value = conn.call("profiles.list", Value::Null)?;
        let items = list["items"].as_array().cloned().unwrap_or_default();
        let hits: Vec<_> = items.iter().filter(|p| p["id"] == key || p["name"] == key).collect();
        match hits.as_slice() {
            [one] => Ok(one["id"].as_str().unwrap_or_default().to_owned()),
            [] => bail!("no profile {key:?}"),
            _ => bail!("several profiles are named {key:?}; use the id"),
        }
    };
    match cmd {
        ProfileCmd::List => {
            let r: Value = conn.call("profiles.list", Value::Null)?;
            if json {
                return print_json(&r);
            }
            for p in r["items"].as_array().into_iter().flatten() {
                let cur = if p["id"] == r["current"] { "*" } else { " " };
                let kind = p["source"]["type"].as_str().unwrap_or("?");
                let mut line = format!("{cur} {:<10} {:<24} {:<7} updated {}", p["id"].as_str().unwrap_or(""), p["name"].as_str().unwrap_or(""), kind, ago(p["updated"].as_u64()));
                if let Some(u) = p.get("usage").filter(|u| !u.is_null()) {
                    let used = u["upload"].as_u64().unwrap_or(0) + u["download"].as_u64().unwrap_or(0);
                    line += &format!(" · {} / {}", bytes(used), bytes(u["total"].as_u64().unwrap_or(0)));
                }
                if p["draft"] == true {
                    line += " · draft: paste a config and save";
                }
                if let Some(e) = p["last_error"].as_str() {
                    line += &format!(" · error: {e}");
                }
                println!("{line}");
            }
        }
        ProfileCmd::Add { source, name, user_agent } => {
            let params = if source.starts_with("http://") || source.starts_with("https://") {
                json!({ "url": source, "name": name, "user_agent": user_agent })
            } else {
                let path = std::fs::canonicalize(&source).with_context(|| format!("no such file {source}"))?;
                json!({ "path": path, "name": name })
            };
            let r: Value = conn.call("profiles.add", params)?;
            println!("added {}", r["id"].as_str().unwrap_or("?"));
        }
        ProfileCmd::New { name } => {
            let r: Value = conn.call("profiles.create", json!({ "name": name }))?;
            let path = r["path"].as_str().context("daemon sent no path")?;
            println!("created {} at {path}", r["id"].as_str().unwrap_or("?"));
            edit(path)?;
        }
        ProfileCmd::Edit { profile } => {
            let id = resolve(conn, &profile)?;
            let list: Value = conn.call("profiles.list", Value::Null)?;
            let p = list["items"].as_array().into_iter().flatten().find(|p| p["id"] == id.as_str()).context("profile vanished")?;
            if p["source"]["type"] != "local" {
                bail!("{profile} is a subscription: its file is replaced on every update; edit its overrides instead");
            }
            let root = list["root"].as_str().context("daemon sent no data root")?;
            edit(&format!("{root}/profiles/{id}.yaml"))?;
        }
        ProfileCmd::Update { profile, force } => {
            let id = profile.map(|p| resolve(conn, &p)).transpose()?;
            let r: Value = conn.call("profiles.update", json!({ "id": id, "force": force }))?;
            if json {
                return print_json(&r);
            }
            let list = r.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                println!("nothing to update");
            }
            for u in list {
                let err = u["error"].as_str().map(|e| format!(": {e}")).unwrap_or_default();
                println!("{} {}{err}", u["name"].as_str().unwrap_or("?"), u["outcome"].as_str().unwrap_or("?"));
            }
        }
        ProfileCmd::Use { profile } => {
            let id = resolve(conn, &profile)?;
            conn.call::<Value>("profiles.select", json!({ "id": id }))?;
        }
        ProfileCmd::Rm { profile } => {
            let id = resolve(conn, &profile)?;
            conn.call::<Value>("profiles.remove", json!({ "id": id }))?;
        }
    }
    Ok(())
}

/// Why TUN is off, with what to do about it.
fn tun_reasons(t: &omm_proto::TunIssue) -> Vec<String> {
    let mut out = Vec::new();
    if t.no_capabilities {
        out.push("the core lacks capabilities (omm fix setcap)".into());
    }
    if let Some(app) = &t.other_core {
        out.push(format!("{app} runs its own core: turn its TUN off or omm migrate cvr"));
    }
    if let Some(m) = &t.message {
        out.push(format!("core: {m}"));
    }
    if out.is_empty() {
        out.push("see journalctl --user -u omarchy-mihomo-core".into());
    }
    out
}

/// Run the user's editor on `path` and wait for it.
fn edit(path: &str) -> Result<()> {
    let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "nvim".into());
    let status = std::process::Command::new("sh")
        .args(["-c", &format!("{editor} \"$1\""), "omm", path])
        .status()
        .with_context(|| format!("running {editor}"))?;
    if !status.success() {
        bail!("{editor} exited with {status}");
    }
    println!("saved changes are picked up by the daemon; see `omm profile`");
    Ok(())
}

fn problems(conn: &mut Conn, cmd: ProblemsCmd, json: bool) -> Result<()> {
    match cmd {
        ProblemsCmd::List { new, hidden, kind, app, search, limit } => {
            let r: Value = conn.call("problems.list", json!({ "only_new": new, "hidden": hidden, "kind": kind, "app": app, "search": search, "limit": limit }))?;
            if json {
                return print_json(&r);
            }
            let c = &r["counts"];
            println!("{} new · {} visible · {} hidden", c["new"], c["visible"], c["hidden"]);
            for p in r["items"].as_array().into_iter().flatten() {
                let kinds: Vec<String> = p["kinds"].as_object().into_iter().flatten().map(|(k, n)| format!("{k}×{n}")).collect();
                let apps: Vec<&str> = p["apps"].as_object().into_iter().flatten().map(|(a, _)| a.as_str()).collect();
                let route = &p["route"];
                let rule = route["rule"].as_str().filter(|r| !r.is_empty()).map(|r| format!("{r} → ")).unwrap_or_default();
                let ok = p["ok_since_error"].as_u64().filter(|n| *n > 0).map(|n| format!(" · ok×{n} since")).unwrap_or_default();
                let host = match p["domain_hint"].as_str() {
                    Some(h) => format!("{} ({h})", p["host"].as_str().unwrap_or("")),
                    None => p["host"].as_str().unwrap_or("").to_owned(),
                };
                println!(
                    "{} {:<34} ×{:<5} {:<22} {:<20} {}{}  {}{ok}",
                    if p["new"] == true { "●" } else { " " },
                    host,
                    p["count"],
                    kinds.join(","),
                    if apps.is_empty() { "-".into() } else { apps.join(",") },
                    rule,
                    route["target"].as_str().unwrap_or(""),
                    ago(p["last_seen"].as_u64().map(|ms| ms / 1000)),
                );
            }
        }
        ProblemsCmd::Seen => {
            conn.call::<Value>("problems.mark_seen", Value::Null)?;
        }
        ProblemsCmd::Hide { value, suffix, app } => {
            let kind = if app { "app" } else if suffix { "suffix" } else { "host" };
            let r: Value = conn.call("problems.hide", json!({ "kind": kind, "value": value }))?;
            println!("hidden #{}", r["id"]);
        }
        ProblemsCmd::Ignores => {
            let r: Value = conn.call("problems.ignores", Value::Null)?;
            if json {
                return print_json(&r);
            }
            for i in r.as_array().into_iter().flatten() {
                println!("#{:<4} {:<7} {}", i["id"], i["kind"].as_str().unwrap_or(""), i["value"].as_str().unwrap_or(""));
            }
        }
        ProblemsCmd::Unhide { id } => {
            conn.call::<Value>("problems.unhide", json!({ "id": id }))?;
        }
        ProblemsCmd::Clear { host } => {
            let r: Value = conn.call("problems.clear", json!({ "host": host }))?;
            println!("removed {}", r["removed"]);
        }
    }
    Ok(())
}

fn rules(conn: &mut Conn, cmd: RulesCmd, json: bool) -> Result<()> {
    let applied = |r: &Value| {
        let mut s = String::new();
        if r["apply"]["reloaded"] == true {
            s += "applied";
        } else {
            s += "saved (core not reloaded)";
        }
        if let Some(n) = r["closed"].as_u64().filter(|n| *n > 0) {
            s += &format!(", closed {n} connection(s)");
        }
        s
    };
    match cmd {
        RulesCmd::List => {
            let r: Value = conn.call("rules.list", Value::Null)?;
            if json {
                return print_json(&r);
            }
            for x in r["items"].as_array().into_iter().flatten() {
                let state = if x["orphan"] == true {
                    "orphan"
                } else if x["enabled"] == true {
                    "on"
                } else {
                    "off"
                };
                let hits = x["hits"].as_u64().map(|h| format!("{h} hits")).unwrap_or_default();
                println!("#{:<4} {:<6} {:<48} {:<10} {}", x["id"], state, x["line"].as_str().unwrap_or(""), hits, x["source"].as_str().filter(|s| *s != "user").unwrap_or(""));
            }
        }
        RulesCmd::Add { value, via, exact, check, note } => {
            let is_ip = value.trim_matches(['[', ']']).split('/').next().is_some_and(|v| v.parse::<std::net::IpAddr>().is_ok());
            let kind = if is_ip { "ip-cidr" } else if exact { "domain" } else { "domain-suffix" };
            let preview: Value = conn.call("rules.preview", json!({ "kind": kind, "value": value, "target": via }))?;
            let mut test = Value::Null;
            if check {
                let host = preview["value"].as_str().unwrap_or("").split('/').next().unwrap_or("").to_owned();
                let r: Value = conn.call("rules.test", json!({ "host": host, "targets": [via] }))?;
                let o = &r["results"][0];
                if o["ok"] != true {
                    bail!("{} via {via} failed: {} — not saved", r["probe"]["url"].as_str().unwrap_or(""), o["error"].as_str().unwrap_or("?"));
                }
                println!("check ok: {} ms", o["delay"]);
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
                test = json!({ "target": via, "ok": true, "delay": o["delay"], "at": now });
            }
            let r: Value = conn.call("rules.save", json!({ "kind": kind, "value": value, "target": via, "note": note, "test": test }))?;
            println!("{} — {}", preview["line"].as_str().unwrap_or(""), applied(&r));
        }
        RulesCmd::Test { host, targets, port } => {
            let r: Value = conn.call("rules.test", json!({ "host": host, "port": port, "targets": targets }))?;
            if json {
                return print_json(&r);
            }
            println!("{}{}", r["probe"]["url"].as_str().unwrap_or(""), if r["probe"]["inconclusive"] == true { "  (a failure here may not mean the service is down)" } else { "" });
            for o in r["results"].as_array().into_iter().flatten() {
                match o["ok"].as_bool() {
                    Some(true) => println!("  {:<24} {} ms", o["target"].as_str().unwrap_or(""), o["delay"]),
                    _ => println!("  {:<24} failed: {}", o["target"].as_str().unwrap_or(""), o["error"].as_str().unwrap_or("?")),
                }
            }
        }
        RulesCmd::Set { id, via, on, off } => {
            let enabled = if on { Some(true) } else if off { Some(false) } else { None };
            let r: Value = conn.call("rules.update", json!({ "id": id, "target": via, "enabled": enabled }))?;
            println!("{} — {}", r["rule"]["value"].as_str().unwrap_or(""), applied(&r));
        }
        RulesCmd::Rm { id } => {
            let r: Value = conn.call("rules.delete", json!({ "id": id }))?;
            println!("deleted — {}", applied(&r));
        }
        RulesCmd::History { limit } => {
            let r: Value = conn.call("rules.history", json!({ "limit": limit }))?;
            if json {
                return print_json(&r);
            }
            for c in r.as_array().into_iter().flatten() {
                let describe = |s: &Value| if s.is_null() { "—".to_owned() } else { format!("{} → {}{}", s["value"].as_str().unwrap_or(""), s["target"].as_str().unwrap_or(""), if s["enabled"] == false { " (off)" } else { "" }) };
                let reverts = c["reverts"].as_i64().map(|r| format!(" (undo #{r})")).unwrap_or_default();
                println!("#{:<4} {:<7} {:<12} {}  ⇒  {}{reverts}", c["id"], c["action"].as_str().unwrap_or(""), ago(c["at"].as_u64().map(|ms| ms / 1000)), describe(&c["before"]), describe(&c["after"]));
            }
        }
        RulesCmd::Rollback { to } => {
            let r: Value = conn.call("rules.rollback", json!({ "to": to }))?;
            println!("undid {} change(s) — {}", r["undone"], applied(&r));
        }
    }
    Ok(())
}

fn ago(t: Option<u64>) -> String {
    let Some(t) = t else { return "never".into() };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    match now.saturating_sub(t) {
        s if s < 90 => "just now".into(),
        s if s < 5400 => format!("{} min ago", s / 60),
        s if s < 172_800 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86400),
    }
}

fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", UNITS[i]) }
}

fn fmt_delay(d: Option<u32>) -> String {
    match d {
        None => "—".into(),
        Some(0) => "timeout".into(),
        Some(ms) => format!("{ms} ms"),
    }
}

fn print_json(v: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

struct Conn {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
    /// Events that arrived while waiting for a response.
    pending: VecDeque<String>,
}

impl Conn {
    fn open(path: &PathBuf) -> Result<Self> {
        let stream = UnixStream::connect(path).with_context(|| {
            format!("cannot connect to {} — is omarchy-mihomod running? (systemctl --user start omarchy-mihomod)", path.display())
        })?;
        Ok(Self { reader: BufReader::new(stream.try_clone()?), writer: stream, next_id: 1, pending: VecDeque::new() })
    }

    fn read_line(&mut self) -> Result<String> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            bail!("daemon closed the connection");
        }
        Ok(line.trim_end().to_owned())
    }

    fn call<T: DeserializeOwned>(&mut self, method: &str, params: Value) -> Result<T> {
        let id = self.next_id;
        self.next_id += 1;
        let mut frame = serde_json::to_vec(&Request { id, method: method.into(), params })?;
        frame.push(b'\n');
        self.writer.write_all(&frame)?;

        loop {
            let line = self.read_line()?;
            match serde_json::from_str::<ServerFrame>(&line)? {
                ServerFrame::Response(r) if r.id == id => {
                    if let Some(e) = r.error {
                        bail!("{}", e.message);
                    }
                    return Ok(serde_json::from_value(r.result.unwrap_or(Value::Null))?);
                }
                ServerFrame::Event(_) => self.pending.push_back(line),
                ServerFrame::Response(_) => {}
            }
        }
    }
}
