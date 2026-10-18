use anyhow::{Result, bail};
use serde_json::{Value, json};
pub fn core_install(_: Option<&str>, _: Option<&str>) -> Result<()> {
    bail!(
        "Run packaging/macos/build.sh and packaging/macos/install.sh to update the installed core"
    )
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
