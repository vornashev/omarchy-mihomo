//! `omm`: thin client for `omarchy-mihomod`.

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
    /// Stream events (status, traffic) as JSON lines.
    Watch { topics: Vec<String> },
    /// Call any daemon method: `omm call proxies.delay '{"name":"AI"}'`.
    Call { method: String, params: Option<String> },
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
