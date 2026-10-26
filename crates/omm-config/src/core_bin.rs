//! Installing the mihomo core binary from MetaCubeX releases.
//!
//! The default version is pinned with its SHA-256 so an install is
//! reproducible without trusting the API. Other versions take their digest
//! from the GitHub release metadata. Releases are fetched by tag: the
//! repository's default branch no longer holds the Go sources.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

pub const PINNED_VERSION: &str = "v1.19.32";
const REPO: &str = "MetaCubeX/mihomo";

/// `(asset, sha256)` for the pinned version.
const PINNED: &[(&str, &str)] = &[
    ("mihomo-darwin-arm64-v1.19.32.gz", "3312a6780652c622890fd4357c6a853bbf865464fd047ac7b7f52dab8de18652"),
    ("mihomo-linux-amd64-v1.19.32.gz", "8451100836c9eda194331c2babfad490b2faf30cebc1a04b0e76fd8ac2d35d10"),
    ("mihomo-linux-amd64-compatible-v1.19.32.gz", "ba3ce607747a07f948fc35780e108a4a7c7f552a38b9bd4d115f313ebcb89c20"),
    ("mihomo-linux-arm64-v1.19.32.gz", "9dd862e28b46ff7d775f169cceebc28deccaa0a9e804237d421cd2571e0caba0"),
];

/// Capabilities TUN needs; applied with `setcap` after every install.
pub const CAPABILITIES: &str = "cap_net_admin,cap_net_bind_service,cap_net_raw=ep";

#[derive(Debug, Clone, serde::Serialize)]
pub struct Release {
    pub version: String,
    pub asset: String,
    pub url: String,
    pub sha256: String,
}

/// `~/.local/lib/omarchy-mihomo/mihomo`.
pub fn default_path() -> PathBuf {
    omm_platform::core_binary()
}

/// `mihomo.prev` next to `dest`: the binary an install replaced.
pub fn previous_path(dest: &Path) -> PathBuf {
    dest.with_file_name("mihomo.prev")
}

/// `(major, minor, patch)` of a release tag: `v1.19.32`, `1.19.32`.
/// Alpha and other builds have none.
pub fn release_version(tag: &str) -> Option<(u32, u32, u32)> {
    let mut parts = tag.trim().trim_start_matches('v').split('.');
    let mut next = || parts.next()?.parse().ok();
    let v = (next()?, next()?, next()?);
    parts.next().is_none().then_some(v)
}

/// Whether release `latest` is newer than the running `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (release_version(latest), release_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Release asset for this machine. Plain `amd64` builds need x86-64-v3
/// (AVX2); older CPUs get the `compatible` build.
pub fn asset_name(version: &str) -> Result<String> {
    if omm_platform::IS_MACOS {
        if std::env::consts::ARCH != "aarch64" { return Err(Error::Invalid("macOS builds require Apple Silicon".into())); }
        return Ok(format!("mihomo-darwin-arm64-{version}.gz"));
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" if cpu_has_v3() => "amd64",
        "x86_64" => "amd64-compatible",
        "aarch64" => "arm64",
        other => return Err(Error::Invalid(format!("no mihomo build known for {other}"))),
    };
    Ok(format!("mihomo-linux-{arch}-{version}.gz"))
}

fn cpu_has_v3() -> bool {
    std::fs::read_to_string("/proc/cpuinfo")
        .map(|c| ["avx2", "bmi2", "fma"].iter().all(|f| c.split_whitespace().any(|w| w == *f)))
        .unwrap_or(false)
}

fn client(proxy: Option<&str>) -> Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("omarchy-mihomo/", env!("CARGO_PKG_VERSION")))
        .no_proxy();
    if let Some(p) = proxy {
        b = b.proxy(reqwest::Proxy::all(p).map_err(|e| Error::Fetch(e.to_string()))?);
    }
    b.build().map_err(|e| Error::Fetch(e.to_string()))
}

/// Where to get `version` (`None` = pinned, `"latest"` = newest stable).
pub async fn resolve(version: Option<&str>, proxy: Option<&str>) -> Result<Release> {
    let version = version.unwrap_or(PINNED_VERSION);
    if version == PINNED_VERSION {
        let asset = asset_name(version)?;
        if let Some((_, sha)) = PINNED.iter().find(|(a, _)| *a == asset) {
            return Ok(Release {
                url: format!("https://github.com/{REPO}/releases/download/{version}/{asset}"),
                version: version.into(),
                asset,
                sha256: (*sha).into(),
            });
        }
    }
    let api = if version == "latest" {
        format!("https://api.github.com/repos/{REPO}/releases/latest")
    } else {
        format!("https://api.github.com/repos/{REPO}/releases/tags/{version}")
    };
    let text = client(proxy)?
        .get(&api)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| Error::Fetch(e.to_string()))?
        .text()
        .await
        .map_err(|e| Error::Fetch(e.to_string()))?;
    let rel: serde_json::Value = serde_json::from_str(&text).map_err(|e| Error::Fetch(e.to_string()))?;
    let tag = rel["tag_name"].as_str().ok_or_else(|| Error::Fetch("release has no tag".into()))?.to_owned();
    if rel["prerelease"] == true {
        return Err(Error::Invalid(format!("{tag} is a prerelease")));
    }
    let asset = asset_name(&tag)?;
    let entry = rel["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"] == asset.as_str())
        .ok_or_else(|| Error::Fetch(format!("{tag} has no {asset}")))?;
    let sha = entry["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or_else(|| Error::Fetch(format!("{asset} has no published sha256; refusing to install it unverified")))?;
    Ok(Release {
        version: tag,
        asset,
        url: entry["browser_download_url"].as_str().unwrap_or_default().to_owned(),
        sha256: sha.into(),
    })
}

/// Download, verify, unpack, smoke-test and atomically replace `dest`.
/// Returns the version line the new binary prints.
pub async fn install(rel: &Release, dest: &Path, proxy: Option<&str>) -> Result<String> {
    install_with_progress(rel, dest, proxy, |_, _| {}).await
}

pub async fn install_with_progress(rel: &Release, dest: &Path, proxy: Option<&str>, mut progress: impl FnMut(u64, Option<u64>) + Send) -> Result<String> {
    let mut response = client(proxy)?
        .get(&rel.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| Error::Fetch(e.to_string()))?;
    let total = response.content_length();
    let mut bytes = Vec::new();
    let mut reported = std::time::Instant::now();
    progress(0, total);
    while let Some(chunk) = response.chunk().await.map_err(|e| Error::Fetch(e.to_string()))? {
        bytes.extend_from_slice(&chunk);
        if reported.elapsed() >= Duration::from_millis(100) {
            progress(bytes.len() as u64, total);
            reported = std::time::Instant::now();
        }
    }
    progress(bytes.len() as u64, total);
    let got = hex(&Sha256::digest(&bytes));
    if !got.eq_ignore_ascii_case(&rel.sha256) {
        return Err(Error::Invalid(format!("{} checksum mismatch: expected {}, got {got}", rel.asset, rel.sha256)));
    }
    let mut binary = Vec::new();
    flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut binary).map_err(|e| Error::Invalid(format!("unpack {}: {e}", rel.asset)))?;

    let dir = dest.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let staged = dir.join(".mihomo.new");
    std::fs::write(&staged, &binary).map_err(|e| Error::io(&staged, e))?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| Error::io(&staged, e))?;
    let version = match binary_version(&staged).await {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&staged);
            return Err(e);
        }
    };
    // A hard link keeps the old binary's inode, and with it the file
    // capabilities TUN needs, for `rollback`.
    if dest.is_file() {
        let prev = previous_path(dest);
        let _ = std::fs::remove_file(&prev);
        if let Err(e) = std::fs::hard_link(dest, &prev) {
            tracing::warn!("keeping the previous core: {e}");
        }
    }
    std::fs::rename(&staged, dest).map_err(|e| Error::io(dest, e))?;
    Ok(version)
}

/// Put back the binary the last install replaced.
pub fn rollback(dest: &Path) -> Result<()> {
    let prev = previous_path(dest);
    if !prev.is_file() {
        return Err(Error::Invalid("no previous core to go back to".into()));
    }
    std::fs::rename(&prev, dest).map_err(|e| Error::io(dest, e))
}

/// First line of `mihomo -v`.
pub async fn binary_version(bin: &Path) -> Result<String> {
    let out = tokio::time::timeout(Duration::from_secs(10), tokio::process::Command::new(bin).arg("-v").output())
        .await
        .map_err(|_| Error::Invalid(format!("{} -v timed out", bin.display())))?
        .map_err(|e| Error::io(bin, e))?;
    let line = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default().trim().to_owned();
    if !out.status.success() || !line.contains("Mihomo") {
        return Err(Error::Invalid(format!("{} does not look like mihomo: {line}", bin.display())));
    }
    Ok(line)
}

/// Whether the binary carries the capabilities TUN needs.
pub fn has_capabilities(bin: &Path) -> Option<bool> {
    if omm_platform::IS_MACOS { return None; }
    let out = std::process::Command::new("getcap").arg(bin).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    Some(["cap_net_admin", "cap_net_raw", "cap_net_bind_service"].iter().all(|c| s.contains(c)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_assets_have_hashes() {
        let asset = asset_name(PINNED_VERSION).unwrap();
        assert!(PINNED.iter().any(|(a, _)| *a == asset), "{asset} not pinned");
        for (_, h) in PINNED {
            assert_eq!(h.len(), 64);
        }
    }

    #[test]
    fn versions() {
        assert_eq!(release_version("v1.19.32"), Some((1, 19, 32)));
        assert_eq!(release_version("1.20.0"), Some((1, 20, 0)));
        assert_eq!(release_version("alpha-5d3b2c1"), None);
        assert_eq!(release_version("v1.19.32-alpha"), None);
        assert!(is_newer("v1.20.0", "v1.19.32"));
        assert!(is_newer("v1.19.33", "1.19.32"));
        assert!(!is_newer("v1.19.32", "v1.19.32"));
        assert!(!is_newer("v1.19.31", "v1.19.32"));
        assert!(!is_newer("v1.20.0", "alpha-5d3b2c1"), "custom builds are left alone");
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(hex(&Sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[tokio::test]
    async fn streamed_install_reports_bytes_and_keeps_checksum_validation() {
        use std::io::Write;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gzip.write_all(b"#!/bin/sh\necho 'Mihomo test'\n").unwrap();
        let bytes = gzip.finish().unwrap();
        for (chunked, valid_hash) in [(false, true), (true, true), (false, false)] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/core.gz", listener.local_addr().unwrap());
            let body = bytes.clone();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") { request.push(socket.read_u8().await.unwrap()); }
                let headers = if chunked { "Transfer-Encoding: chunked".into() } else { format!("Content-Length: {}", body.len()) };
                socket.write_all(format!("HTTP/1.1 200 OK\r\n{headers}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
                if chunked { socket.write_all(format!("{:x}\r\n", body.len()).as_bytes()).await.unwrap(); }
                socket.write_all(&body).await.unwrap();
                if chunked { socket.write_all(b"\r\n0\r\n\r\n").await.unwrap(); }
            });
            let dir = tempfile::tempdir().unwrap();
            let dest = dir.path().join("mihomo");
            std::fs::write(&dest, "previous binary").unwrap();
            let rel = Release { version: "test".into(), asset: "test.gz".into(), url, sha256: if valid_hash { hex(&Sha256::digest(&bytes)) } else { "bad".into() } };
            let mut progress = Vec::new();
            let result = install_with_progress(&rel, &dest, None, |n, total| progress.push((n, total))).await;
            server.await.unwrap();
            let total = (!chunked).then_some(bytes.len() as u64);
            assert_eq!(progress.first(), Some(&(0, total)));
            assert_eq!(progress.last(), Some(&(bytes.len() as u64, total)));
            if valid_hash {
                assert_eq!(result.unwrap(), "Mihomo test");
                assert_eq!(std::fs::read_to_string(previous_path(&dest)).unwrap(), "previous binary");
                rollback(&dest).unwrap();
                assert_eq!(std::fs::read_to_string(&dest).unwrap(), "previous binary");
                assert!(rollback(&dest).is_err(), "only one step back");
            } else {
                assert!(result.unwrap_err().to_string().contains("checksum mismatch"));
                assert_eq!(std::fs::read_to_string(&dest).unwrap(), "previous binary");
            }
        }
    }
}
