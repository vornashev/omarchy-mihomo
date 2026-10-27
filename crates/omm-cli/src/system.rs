//! Commands that work without the daemon: core binary management and doctor.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use omm_config::core_bin;
use omm_config::paths::Paths;
use serde_json::Value;

pub use omm_config::host::{CORE_UNIT, DAEMON_UNIT};
const PLUGIN_ID: &str = "tema.omarchy-mihomo";

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread().enable_all().build()?)
}

/// Download and install the core; `version` None = pinned, "latest" = newest.
pub fn core_install(version: Option<&str>, proxy: Option<&str>, dest: Option<PathBuf>) -> Result<()> {
    // Only the core the unit runs needs TUN capabilities.
    let ours = dest.is_none();
    let dest = dest.unwrap_or_else(core_bin::default_path);
    let rt = runtime()?;
    let rel = rt.block_on(core_bin::resolve(version, proxy))?;
    println!("downloading {} ({})", rel.asset, rel.version);
    let installed = rt.block_on(core_bin::install(&rel, &dest, proxy))?;
    println!("installed {} → {}", installed, dest.display());
    println!("sha256 {} verified", rel.sha256);
    if ours && core_bin::has_capabilities(&dest) != Some(true) {
        println!("TUN needs capabilities on the new binary: run `omm core setcap`");
    }
    Ok(())
}

/// `sudo setcap` on our core binary. Interactive: sudo asks for a password.
pub fn core_setcap() -> Result<()> {
    let bin = core_bin::default_path();
    if !bin.is_file() {
        bail!("{} not found; run `omm core install` first", bin.display());
    }
    println!("sudo setcap {} {}", core_bin::CAPABILITIES, bin.display());
    let status = Command::new("sudo").arg("setcap").arg(core_bin::CAPABILITIES).arg(&bin).status().context("run sudo")?;
    if !status.success() {
        bail!("setcap failed");
    }
    println!("ok: {}", getcap(&bin));
    Ok(())
}

/// The previous binary keeps its capabilities: it is the same file.
pub fn core_rollback() -> Result<()> {
    let bin = core_bin::default_path();
    core_bin::rollback(&bin)?;
    let rt = runtime()?;
    println!("restored {}", rt.block_on(core_bin::binary_version(&bin))?);
    if omm_config::host::unit_active(true, CORE_UNIT) {
        core_unit("restart")?;
    }
    Ok(())
}

pub fn core_unit(verb: &str) -> Result<()> {
    let status = Command::new("systemctl").args(["--user", verb, CORE_UNIT]).status()?;
    if !status.success() {
        bail!("systemctl --user {verb} {CORE_UNIT} failed");
    }
    Ok(())
}

fn getcap(bin: &Path) -> String {
    Command::new("getcap").arg(bin).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
}

fn systemctl(args: &[&str]) -> String {
    Command::new("systemctl").args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

// ---------------------------------------------------------------- doctor

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Ok,
    Warn,
    Fail,
}

struct Report {
    fails: usize,
    warns: usize,
}

impl Report {
    fn line(&mut self, level: Level, text: impl AsRef<str>, fix: Option<&str>) {
        let mark = match level {
            Level::Ok => "\x1b[32m✓\x1b[0m",
            Level::Warn => {
                self.warns += 1;
                "\x1b[33m!\x1b[0m"
            }
            Level::Fail => {
                self.fails += 1;
                "\x1b[31m✗\x1b[0m"
            }
        };
        println!("{mark} {}", text.as_ref());
        if let Some(f) = fix {
            println!("    → {f}");
        }
    }
    fn section(&self, name: &str) {
        println!("\n\x1b[1m{name}\x1b[0m");
    }
}

/// Check everything that has to line up for omarchy-mihomo to work, and
/// say how to fix what does not. `daemon` is the daemon's status, if reachable.
pub fn doctor(daemon: Option<Value>) -> Result<bool> {
    let mut r = Report { fails: 0, warns: 0 };
    let paths = Paths::new(Paths::default_root());

    r.section("Programs");
    for (name, path) in [("omm", home().join(".local/bin/omm")), ("omarchy-mihomod", home().join(".local/bin/omarchy-mihomod"))] {
        if path.is_file() {
            r.line(Level::Ok, format!("{name} installed"), None);
        } else {
            r.line(Level::Fail, format!("{name} missing at {}", path.display()), Some("run packaging/install.sh"));
        }
    }
    let bin = core_bin::default_path();
    if bin.is_file() {
        let version = runtime().ok().and_then(|rt| rt.block_on(core_bin::binary_version(&bin)).ok());
        match version {
            Some(v) => r.line(Level::Ok, format!("core: {v}"), None),
            None => r.line(Level::Fail, format!("core at {} does not run", bin.display()), Some("omm core install")),
        }
        match core_bin::has_capabilities(&bin) {
            Some(true) => r.line(Level::Ok, "core has TUN capabilities", None),
            _ => r.line(Level::Fail, "core lacks TUN capabilities (cap_net_admin, cap_net_raw, cap_net_bind_service)", Some("omm core setcap")),
        }
    } else {
        r.line(Level::Fail, format!("core binary missing at {}", bin.display()), Some("omm core install"));
    }

    r.section("Services");
    for (unit, required) in [(DAEMON_UNIT, true), (CORE_UNIT, true)] {
        let installed = home().join(".config/systemd/user").join(unit).is_file();
        if !installed {
            r.line(Level::Fail, format!("{unit} not installed"), Some("run packaging/install.sh"));
            continue;
        }
        let enabled = systemctl(&["--user", "is-enabled", unit]);
        let active = systemctl(&["--user", "is-active", unit]);
        let level = if active == "active" { Level::Ok } else if required { Level::Warn } else { Level::Ok };
        let fix = (active != "active").then_some(if unit == CORE_UNIT { "omm core start  (or, from Clash Verge: omm migrate cvr)" } else { "systemctl --user enable --now omarchy-mihomod" });
        r.line(level, format!("{unit}: {active}, {enabled}"), fix);
    }

    r.section("Daemon and core");
    match &daemon {
        Some(s) => {
            r.line(Level::Ok, format!("daemon {} answers", s["daemon_version"].as_str().unwrap_or("?")), None);
            let core = &s["core"];
            if core["running"] == true {
                let socket = core["socket"].as_str().unwrap_or("");
                let ours = Path::new(socket) == paths.controller_socket();
                r.line(
                    if ours { Level::Ok } else { Level::Warn },
                    format!("core {} · mode {} · TUN {}", core["version"].as_str().unwrap_or("?"), core["mode"].as_str().unwrap_or("?"), if core["tun"] == true { "on" } else { "off" }),
                    (!ours).then_some("the daemon watches another core (OMM_CORE_SOCKET); configs are not applied to it"),
                );
                if let Some(t) = core.get("tun_issue").filter(|t| !t.is_null()) {
                    let fix = if t["no_capabilities"] == true { "omm fix setcap" } else { "omm migrate cvr" };
                    r.line(Level::Warn, format!("TUN is on in the settings but the core could not start it{}", t["message"].as_str().map(|m| format!(": {m}")).unwrap_or_default()), Some(fix));
                }
            } else {
                r.line(Level::Warn, "core not running", Some("omm core start"));
            }
            match s["config"]["profile"]["name"].as_str() {
                Some(p) => r.line(Level::Ok, format!("profile: {p}"), None),
                None => r.line(Level::Warn, "no profile", Some("omm profile add <subscription URL>  or  omm migrate cvr")),
            }
            if let Some(e) = s["config"]["error"].as_str() {
                r.line(Level::Fail, format!("last apply failed: {}", e.lines().next().unwrap_or(e)), Some("omm apply"));
            }
            if let Some(o) = s["config"]["orphans"].as_array().filter(|o| !o.is_empty()) {
                r.line(Level::Warn, format!("{} rule(s) point at groups missing from the profile", o.len()), Some("omm rules list"));
            }
        }
        None => r.line(Level::Fail, "daemon not reachable", Some("systemctl --user enable --now omarchy-mihomod")),
    }
    if paths.runtime_config().is_file() {
        r.line(Level::Ok, format!("runtime config {}", paths.runtime_config().display()), None);
    } else {
        r.line(Level::Warn, "no runtime config yet", Some("omm apply (after adding a profile)"));
    }

    r.section("Conflicts");
    let cvr_service = systemctl(&["is-active", "clash-verge-service.service"]);
    let cvr_core = Command::new("pgrep").args(["-x", "verge-mihomo"]).output().map(|o| o.status.success()).unwrap_or(false);
    if cvr_service == "active" || cvr_core {
        let ours = systemctl(&["--user", "is-active", CORE_UNIT]) == "active";
        r.line(
            if ours { Level::Fail } else { Level::Warn },
            format!("Clash Verge Rev is running (service {cvr_service}, core {})", if cvr_core { "up" } else { "down" }),
            Some(if ours { "two cores with TUN fight over routes: omm migrate cvr" } else { "omm migrate cvr  moves profiles and switches over" }),
        );
    } else {
        r.line(Level::Ok, "Clash Verge Rev not running", None);
    }
    if systemctl(&["--user", "is-active", "site-autotune-tuner.timer"]) == "active" {
        r.line(Level::Warn, "site-autotune timer is active and rewrites routes on its own", Some("omm fix disable_autotune"));
    }
    let tuns = Command::new("ip").args(["-o", "link", "show", "type", "tun"]).output().map(|o| String::from_utf8_lossy(&o.stdout).lines().count()).unwrap_or(0);
    if tuns > 1 {
        r.line(Level::Warn, format!("{tuns} TUN devices present; only one proxy should own routing"), None);
    }

    r.section("Shell plugin");
    let plugin = home().join(".config/omarchy/plugins").join(PLUGIN_ID).join("manifest.json");
    if plugin.is_file() {
        r.line(Level::Ok, "plugin installed", None);
    } else {
        r.line(Level::Warn, "plugin not installed", Some("run packaging/install.sh"));
    }
    let shell_json = std::fs::read_to_string(home().join(".config/omarchy/shell.json")).unwrap_or_default();
    if shell_json.contains(PLUGIN_ID) {
        r.line(Level::Ok, "plugin is in the bar", None);
    } else if plugin.is_file() {
        r.line(Level::Warn, "plugin not in the bar", Some(&format!("omarchy plugin enable {PLUGIN_ID} --section right")));
    }
    for other in ["io.github.lijiawei0305-pixel.mihomo", "jackzasian.clash-verge"] {
        if shell_json.contains(other) {
            r.line(Level::Warn, format!("{other} is also in the bar"), Some(&format!("omarchy plugin disable {other}")));
        }
    }

    println!("\n{} problem(s), {} warning(s)", r.fails, r.warns);
    Ok(r.fails == 0)
}
