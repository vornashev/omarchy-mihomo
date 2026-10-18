//! macOS snapshots and privileged changes live in the helper, so recovery
//! also works after a daemon crash. User intent stays in the user's data tree.
use anyhow::Result;
use omm_proto::SystemProxyStatus;
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct Manager {
    path: PathBuf,
    wanted: bool,
}
impl Manager {
    pub fn open(path: PathBuf) -> Result<Self> {
        let wanted = omm_config::paths::read_optional(&path)?
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .is_some_and(|v| v["wanted"] == true);
        Ok(Self { path, wanted })
    }
    pub async fn set(&mut self, enable: bool, port: u16) -> Result<SystemProxyStatus> {
        let result =
            omm_platform::helper::call("proxy.set", json!({"enable":enable,"port":port})).await?;
        self.wanted = enable;
        omm_config::paths::write_atomic(
            &self.path,
            &serde_json::to_vec(&json!({"wanted":enable}))?,
        )?;
        Ok(self.status(result, port))
    }
    fn status(&self, value: Value, port: u16) -> SystemProxyStatus {
        SystemProxyStatus {
            available: true,
            enabled: value["enabled"] == true,
            wanted: self.wanted,
            port,
            error: value["error"].as_str().map(Into::into),
        }
    }
    pub async fn refresh(&mut self, port: u16) -> SystemProxyStatus {
        let result = omm_platform::helper::call(
            "proxy.sync",
            json!({"enable":self.wanted && port != 0,"port":port}),
        )
        .await;
        match result {
            Ok(value) => self.status(value, port),
            Err(e) => SystemProxyStatus {
                wanted: self.wanted,
                port,
                error: Some(e.to_string()),
                ..Default::default()
            },
        }
    }
    pub async fn shutdown(&mut self) -> Result<()> {
        omm_platform::helper::call("proxy.set", json!({"enable":false,"port":0})).await?;
        Ok(())
    }
}
