//! `omm fix` and `omm migrate`: the fixes the shell offers, including the
//! ones that need sudo and therefore run in a terminal.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use omm_config::core_bin;
use omm_config::host::{self, AUTOTUNE_COLLECTOR, AUTOTUNE_TIMER, CORE_UNIT, CVR_APP, CVR_SERVICE};
use omm_config::paths::Paths;
use omm_proto::{Issue, Status};
use serde_json::Value;

use crate::{Conn, system};

fn say(text: &str) {
    println!("\x1b[1m==>\x1b[0m {text}");
}

/// English wording of an issue, for the terminal.
fn describe(i: &Issue) -> String {
    let d = i.detail.as_deref().unwrap_or("");
    match i.id.as_str() {
        "core_missing" => "the mihomo core is not installed".into(),
        "core_installing" => "the mihomo core is being downloaded".into(),
        "core_install_failed" => format!("downloading the mihomo core failed: {d}"),
        "other_core" => format!("{d} runs its own core: two cores fight over TUN and routes"),
        "core_no_caps" => "TUN is on, but the core lacks the capabilities to open it".into(),
        "core_stopped" => "the core is not running".into(),
        "tun_failed" => format!("TUN is on, but the core could not start it: {d}"),
        "no_profile" => "no profile yet".into(),
        "autotune" => "the site-autotune timer still rewrites Clash Verge's rules".into(),
        "core_update" => format!("mihomo {d} is out"),
        other => other.into(),
    }
}

pub fn run(socket: &PathBuf, id: Option<&str>) -> Result<()> {
    match id {
        None => list(socket),
        Some("setcap") => setcap(socket),
        Some("update_core") => update_core(socket),
        Some("migrate_cvr") => migrate_cvr(socket),
        Some(id) => {
            let mut conn = Conn::open(socket)?;
            conn.call::<Value>("health.fix", serde_json::json!({ "id": id }))?;
            println!("done");
            Ok(())
        }
    }
}

fn list(socket: &PathBuf) -> Result<()> {
    let status: Status = Conn::open(socket)?.call("status", Value::Null)?;
    if status.issues.is_empty() {
        println!("nothing to fix");
    }
    for i in &status.issues {
        match &i.fix {
            Some(f) => println!("{:<20} {}  →  omm fix {}", i.id, describe(i), f.id),
            None => println!("{:<20} {}", i.id, describe(i)),
        }
    }
    Ok(())
}

/// Tell the daemon the system changed under it; it may be down, that's fine.
fn refresh(socket: &PathBuf) {
    if let Ok(mut c) = Conn::open(socket) {
        let _ = c.call::<Value>("health.refresh", Value::Null);
    }
}

fn setcap(socket: &PathBuf) -> Result<()> {
    system::core_setcap()?;
    if host::unit_active(true, CORE_UNIT) {
        say("restarting the core so it picks up the capabilities");
        host::user_systemctl(&["restart", CORE_UNIT]).map_err(anyhow::Error::msg)?;
    }
    refresh(socket);
    Ok(())
}

/// Install the newest core, give TUN its capabilities back, restart it.
fn update_core(socket: &PathBuf) -> Result<()> {
    let bin = core_bin::default_path();
    let had_caps = core_bin::has_capabilities(&bin) == Some(true);
    if let Err(e) = system::core_install(Some("latest"), None, None) {
        // GitHub may be blocked; the core's own proxy may reach it.
        let status: Option<Status> = Conn::open(socket).ok().and_then(|mut c| c.call("status", Value::Null).ok());
        let port = status.filter(|s| s.core.running).map(|s| s.core.mixed_port).filter(|p| *p > 0);
        let Some(port) = port else { return Err(e) };
        say(&format!("direct download failed ({e:#}), retrying through the core"));
        system::core_install(Some("latest"), Some(&format!("http://127.0.0.1:{port}")), None)?;
    }
    if had_caps {
        system::core_setcap()?;
    }
    if host::unit_active(true, CORE_UNIT) {
        say("restarting the core on the new binary");
        host::user_systemctl(&["restart", CORE_UNIT]).map_err(anyhow::Error::msg)?;
    }
    refresh(socket);
    Ok(())
}

fn sudo_systemctl(args: &[&str]) -> Result<()> {
    println!("sudo systemctl {}", args.join(" "));
    let status = Command::new("sudo").arg("systemctl").args(args).status().context("run sudo")?;
    if !status.success() {
        bail!("sudo systemctl {} failed", args.join(" "));
    }
    Ok(())
}

fn wait_until(what: impl Fn() -> bool, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

fn backup_dir() -> PathBuf {
    Paths::default_root().join("backup")
}

/// Other bar widgets that drive Clash Verge or a second mihomo client.
const OLD_PLUGINS: &[&str] = &["io.github.lijiawei0305-pixel.mihomo", "jackzasian.clash-verge"];

/// What the migration switched off besides Clash Verge itself, so that
/// `--undo` can turn exactly that back on.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct Switched {
    units: Vec<String>,
    plugins: Vec<String>,
}

fn switched_file() -> PathBuf {
    backup_dir().join("migration.json")
}

fn in_bar(plugin: &str) -> bool {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    std::fs::read_to_string(home.join(".config/omarchy/shell.json")).is_ok_and(|s| s.contains(&format!("\"{plugin}\"")))
}

/// Turn off site-autotune (it rewrote Clash Verge's rules on its own; its
/// picks are now plain user rules) and the old proxy widgets in the bar.
fn switch_off_extras(plugins: bool) -> Result<()> {
    let mut done = Switched::default();
    for unit in [AUTOTUNE_TIMER, AUTOTUNE_COLLECTOR] {
        if host::unit_active(true, unit) || host::unit_query(true, "is-enabled", unit) == "enabled" {
            say(&format!("switching off {unit}"));
            host::user_systemctl(&["disable", "--now", unit]).map_err(anyhow::Error::msg)?;
            done.units.push(unit.into());
        }
    }
    for plugin in OLD_PLUGINS.iter().filter(|p| plugins && in_bar(p)) {
        say(&format!("removing {plugin} from the bar"));
        let ok = Command::new("omarchy").args(["plugin", "disable", plugin]).stdout(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
        if ok {
            done.plugins.push((*plugin).into());
        }
    }
    std::fs::create_dir_all(backup_dir())?;
    let mut previous: Switched = std::fs::read_to_string(switched_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    previous.units.extend(done.units);
    previous.plugins.extend(done.plugins);
    previous.units.dedup();
    previous.plugins.dedup();
    std::fs::write(switched_file(), serde_json::to_string_pretty(&previous)?)?;
    Ok(())
}

fn switch_extras_back_on() {
    let Some(done) = std::fs::read_to_string(switched_file()).ok().and_then(|t| serde_json::from_str::<Switched>(&t).ok()) else { return };
    for unit in &done.units {
        say(&format!("switching {unit} back on"));
        let _ = host::user_systemctl(&["enable", "--now", unit]);
    }
    for plugin in &done.plugins {
        say(&format!("putting {plugin} back in the bar"));
        let _ = Command::new("omarchy").args(["plugin", "enable", plugin, "--section", "right"]).stdout(std::process::Stdio::null()).status();
    }
    let _ = std::fs::remove_file(switched_file());
}

/// Move Clash Verge's settings aside and make ours the only proxy:
/// import its profiles, close the app, switch off its root service and
/// autostart, give our core the TUN capabilities and start it.
/// Nothing of Clash Verge is deleted; `omm migrate cvr --undo` restores it.
pub fn migrate_cvr(socket: &PathBuf) -> Result<()> {
    let mut conn = Conn::open(socket)?;

    say("importing Clash Verge profiles");
    let r: Value = conn.call("migrate.cvr_import", Value::Null)?;
    let names = |k: &str| r[k].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    println!("imported: {}", Some(names("imported")).filter(|s| !s.is_empty()).unwrap_or_else(|| "nothing new".into()));
    if !names("skipped").is_empty() {
        println!("already here: {}", names("skipped"));
    }
    if let Some(n) = r["rules"].as_u64().filter(|n| *n > 0) {
        println!("site-autotune rules moved to your rules: {n}");
    }
    for w in r["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        println!("\x1b[33m!\x1b[0m {w}");
    }

    // While Clash Verge still proxies: GitHub may be blocked without it.
    let bin = core_bin::default_path();
    if !bin.is_file() {
        say("installing the mihomo core");
        conn.call::<Value>("health.fix", serde_json::json!({ "id": "install_core" }))?;
    }

    if host::process_running(CVR_APP) {
        say("closing the Clash Verge app");
        let _ = Command::new("pkill").args(["-x", CVR_APP]).status();
        if !wait_until(|| !host::process_running(CVR_APP), Duration::from_secs(5)) {
            let _ = Command::new("pkill").args(["-KILL", "-x", CVR_APP]).status();
        }
    }
    let autostart = host::cvr_autostart();
    if autostart.is_file() {
        say("removing Clash Verge from autostart");
        let dest = backup_dir().join("Clash Verge.desktop");
        std::fs::create_dir_all(backup_dir())?;
        std::fs::rename(&autostart, &dest).with_context(|| format!("move {}", autostart.display()))?;
        println!("kept in {}", dest.display());
    }
    if host::unit_query(false, "is-enabled", CVR_SERVICE) == "enabled" || host::unit_active(false, CVR_SERVICE) {
        say("switching off Clash Verge's service (sudo)");
        sudo_systemctl(&["disable", "--now", CVR_SERVICE])?;
    }
    if !wait_until(|| host::other_core().is_none(), Duration::from_secs(10)) {
        bail!("Clash Verge's core is still running; stop it and run this again");
    }

    if core_bin::has_capabilities(&bin) != Some(true) {
        say("giving the core the TUN capabilities (sudo)");
        system::core_setcap()?;
    }

    say("starting the omarchy-mihomo core");
    conn.call::<Value>("config.apply", Value::Null).context("apply the imported profile")?;
    let verb = if host::unit_active(true, CORE_UNIT) { "restart" } else { "start" };
    host::user_systemctl(&["enable", CORE_UNIT]).map_err(anyhow::Error::msg)?;
    host::user_systemctl(&[verb, CORE_UNIT]).map_err(anyhow::Error::msg)?;
    refresh(socket);

    let up = wait_until(
        || Conn::open(socket).and_then(|mut c| c.call::<Status>("status", Value::Null)).is_ok_and(|s| s.core.running),
        Duration::from_secs(10),
    );
    let status: Status = Conn::open(socket)?.call("status", Value::Null)?;
    if !up {
        bail!("the core did not come up: journalctl --user -u {CORE_UNIT}");
    }
    println!("core running · TUN {}", if status.core.tun { "on" } else { "off" });
    switch_off_extras(true)?;
    println!("\nClash Verge is switched off but not removed. To go back: omm migrate cvr --undo");
    Ok(())
}

/// site-autotune's picks become user rules; the tool itself goes quiet.
pub fn migrate_autotune(socket: &PathBuf) -> Result<()> {
    let mut conn = Conn::open(socket)?;
    let r: Value = conn.call("migrate.autotune", Value::Null)?;
    match r["moved"].as_u64() {
        Some(0) | None => println!("no site-autotune rules left in the profiles' overrides"),
        Some(n) => println!("moved {n} site-autotune rule(s) to your rules (omm rules list)"),
    }
    switch_off_extras(false)?;
    refresh(socket);
    Ok(())
}

/// Hand the machine back to Clash Verge.
pub fn undo_cvr(socket: &PathBuf) -> Result<()> {
    say("stopping the omarchy-mihomo core");
    host::user_systemctl(&["disable", "--now", CORE_UNIT]).map_err(anyhow::Error::msg)?;
    say("switching Clash Verge's service back on (sudo)");
    sudo_systemctl(&["enable", "--now", CVR_SERVICE])?;
    let saved = backup_dir().join("Clash Verge.desktop");
    let autostart = host::cvr_autostart();
    if saved.is_file() && !autostart.exists() {
        say("restoring Clash Verge's autostart");
        if let Some(dir) = autostart.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::rename(&saved, &autostart)?;
    }
    say("starting the Clash Verge app");
    launch_detached(CVR_APP);
    switch_extras_back_on();
    refresh(socket);
    Ok(())
}

fn launch_detached(app: &str) {
    let uwsm = Path::new("/usr/bin/uwsm-app").exists();
    let mut cmd = Command::new("setsid");
    cmd.arg("-f");
    if uwsm {
        cmd.args(["uwsm-app", "--"]);
    }
    let _ = cmd.arg(app).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
}
