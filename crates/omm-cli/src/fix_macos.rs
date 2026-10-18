use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::path::PathBuf;
pub fn run(socket: &PathBuf, id: Option<&str>) -> Result<()> {
    let mut conn = super::Conn::open(socket)?;
    let value: Value = match id {
        Some(id) => conn.call("health.fix", json!({"id":id}))?,
        None => conn.call("status", json!({}))?,
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
pub fn migrate_autotune(_: &PathBuf) -> Result<()> {
    bail!("Linux migration is unavailable on macOS; import YAML or a subscription instead")
}
pub fn migrate_cvr(p: &PathBuf) -> Result<()> {
    migrate_autotune(p)
}
pub fn undo_cvr(p: &PathBuf) -> Result<()> {
    migrate_autotune(p)
}
