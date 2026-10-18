//! Private, bounded NDJSON protocol with the installed network helper.
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use std::time::Duration;

pub const MAX_FRAME: u64 = 8 * 1024 * 1024;
pub const VERSION: u32 = 1;
pub async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Value>> {
    let mut bytes = Vec::new();
    let n = reader.take(MAX_FRAME + 1).read_until(b'\n', &mut bytes).await?;
    if n == 0 { return Ok(None); }
    if n as u64 > MAX_FRAME || bytes.last() != Some(&b'\n') { bail!("invalid or oversized helper frame"); }
    Ok(Some(serde_json::from_slice(&bytes)?))
}
async fn exchange(stream: &mut UnixStream, method: &str, params: Value) -> Result<Value> {
    let mut bytes = serde_json::to_vec(&json!({"version": VERSION, "method":method, "params":params}))?;
    if bytes.len() as u64 >= MAX_FRAME { bail!("helper request too large"); }
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    let response = read_frame(&mut BufReader::new(stream)).await?.context("helper disconnected")?;
    if let Some(error) = response.get("error") { bail!("{}", error.as_str().unwrap_or("helper failed")); }
    Ok(response["result"].clone())
}
pub async fn call(method: &str, params: Value) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(70), async {
        let mut stream = UnixStream::connect(crate::helper_socket()).await.context("Network helper unavailable; run packaging/macos/install.sh")?;
        exchange(&mut stream, method, params).await
    }).await.context("network helper timed out")?
}
/// The helper terminates the core and restores proxy settings when this stream closes.
pub struct Lease(UnixStream);
impl Lease {
    pub async fn acquire() -> Result<Self> {
        let mut stream = UnixStream::connect(crate::helper_socket()).await.context("Network helper unavailable; run packaging/macos/install.sh")?;
        tokio::time::timeout(Duration::from_secs(5), exchange(&mut stream, "lease", Value::Null)).await??;
        Ok(Self(stream))
    }
    pub async fn alive(&mut self) -> bool {
        matches!(self.0.try_read(&mut [0u8; 1]), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
    }
}
