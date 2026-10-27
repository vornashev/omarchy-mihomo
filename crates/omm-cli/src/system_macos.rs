use anyhow::{Result, bail};
use serde_json::{Value, json};
/// The installed core belongs to the privileged helper; `dest` downloads
/// a verified copy elsewhere (the real-core tests on CI).
pub fn core_install(version: Option<&str>, proxy: Option<&str>, dest: Option<std::path::PathBuf>) -> Result<()> {
    let Some(dest) = dest else {
        bail!("Run packaging/macos/build.sh and packaging/macos/install.sh to update the installed core")
    };
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let rel = rt.block_on(omm_config::core_bin::resolve(version, proxy))?;
    println!("downloading {} ({})", rel.asset, rel.version);
    let installed = rt.block_on(omm_config::core_bin::install(&rel, &dest, proxy))?;
    println!("installed {} → {}", installed, dest.display());
    Ok(())
}
pub fn core_rollback() -> Result<()> {
    bail!("macOS uses the network helper; run packaging/macos/install.sh")
}
pub fn core_setcap() -> Result<()> {
    bail!("macOS uses the network helper; run packaging/macos/install.sh")
}
pub fn core_unit(verb: &str) -> Result<()> {
    let socket = std::env::var_os("OMM_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(omm_platform::socket_path);
    let mut conn = super::Conn::open(&socket)?;
    let _: Value = conn.call(&format!("core.{verb}"), json!({}))?;
    Ok(())
}
pub fn doctor(daemon: Option<Value>) -> Result<bool> {
    let Some(status) = daemon else {
        println!("Daemon unavailable. Run the macOS installer.");
        return Ok(false);
    };
    println!(
        "macOS · Apple Silicon\nDaemon: {}\nHelper: {}\nCore: {}",
        status["daemon_version"], status["helper"]["available"], status["core"]["running"]
    );
    if let Some(issues) = status["issues"].as_array() {
        for issue in issues {
            println!("{}: {}", issue["id"], issue["detail"]);
        }
    }
    Ok(status["helper"]["available"] == true && status["core"]["running"] == true)
}
