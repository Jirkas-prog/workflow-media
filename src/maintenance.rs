//! Cooperative shutdown for an external updater. The instance lock remains authoritative.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
pub struct Instance {
    pub pid: u32,
    pub nonce: String,
    pub exe: PathBuf,
    pub preset: PathBuf,
}

pub struct Registration {
    path: PathBuf,
    nonce: String,
}
impl Registration {
    pub fn new(data: &Path, exe: &Path, preset: &Path) -> Result<Self> {
        let path = data.join("running-instance.json");
        let nonce = uuid::Uuid::new_v4().to_string();
        crate::model::save_json(
            &path,
            &Instance {
                pid: std::process::id(),
                nonce: nonce.clone(),
                exe: exe.to_path_buf(),
                preset: preset.to_path_buf(),
            },
        )?;
        Ok(Self { path, nonce })
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        if read_instance(self.path.parent().unwrap()).is_some_and(|i| i.nonce == self.nonce) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub fn read_instance(data: &Path) -> Option<Instance> {
    serde_json::from_slice(&fs::read(data.join("running-instance.json")).ok()?).ok()
}
pub fn shutdown_requested(data: &Path) -> bool {
    let Some(instance) = read_instance(data) else {
        return false;
    };
    if instance.pid != std::process::id() {
        return false;
    }
    let Ok(bytes) = fs::read(data.join("update-stop.json")) else {
        return false;
    };
    let Ok(request) = serde_json::from_slice::<Instance>(&bytes) else {
        return false;
    };
    request.pid == instance.pid && request.nonce == instance.nonce
}
