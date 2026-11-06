//! Passive installation identity; explicit version probes may enrich it.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct InstallationSnapshot {
    path: PathBuf,
    stamp: String,
    pub version: Option<String>,
}

type Versions = HashMap<(PathBuf, String), String>;
static VERSIONS: OnceLock<Mutex<Versions>> = OnceLock::new();

impl InstallationSnapshot {
    pub fn capture(path: &Path) -> Option<Self> {
        let mut digest = Sha256::new();
        let mut version = None;
        let mut files = vec![path.to_path_buf()];
        if path.is_dir() {
            let plist_path = path.join("Contents/Info.plist");
            let info = plist::Value::from_file(&plist_path).ok()?;
            let dictionary = info.as_dictionary()?;
            version = dictionary
                .get("CFBundleShortVersionString")
                .and_then(plist::Value::as_string)
                .map(str::to_owned);
            files.push(plist_path);
            files.push(
                path.join("Contents/MacOS")
                    .join(dictionary.get("CFBundleExecutable")?.as_string()?),
            );
            files.push(path.join("Contents/Resources/app.asar"));
        } else if let Some(parent) = path.parent() {
            files.push(parent.join("resources/app.asar"));
            // Official Windows launcher layout; no process is launched to read it.
            if let Some(root) = parent.parent() {
                if let Ok(data) = fs::read(root.join("updater.cfg")) {
                    if let Ok(config) = serde_json::from_slice::<serde_json::Value>(&data) {
                        if config["currentVersion"]
                            .as_str()
                            .is_some_and(|v| parent.file_name().is_some_and(|name| name == v))
                        {
                            version = config["currentVersion"].as_str().map(str::to_owned);
                        }
                    }
                }
            }
        }
        let resources = if path.is_dir() {
            path.join("Contents/Resources")
        } else {
            path.parent()?.join("resources")
        };
        files.push(resources.join("app.asar.unpacked/node_modules/@qoder-ai/qoder-agent-sdk/dist/_worker/qoder-worker-runtime.obf.mjs"));
        files.push(resources.join("extra-resource/opencode/bin/dumate-opencode"));
        files.push(resources.join("extra-resource/opencode/bin/dumate-opencode.exe"));
        // Include canonical identity and file metadata. A reinstall or active
        // launcher version switch is detected even when its version is unchanged.
        for file in files {
            digest.update(file.to_string_lossy().as_bytes());
            if let Ok(metadata) = fs::metadata(&file) {
                digest.update(
                    format!(
                        "{:?}:{}:{:?}",
                        file.canonicalize().ok(),
                        metadata.len(),
                        metadata.modified().ok()
                    )
                    .as_bytes(),
                );
            }
        }
        fs::metadata(path).ok()?;
        let stamp = format!("{:x}", digest.finalize());
        if let Some(cached) = VERSIONS.get().and_then(|cache| {
            cache
                .lock()
                .ok()?
                .get(&(path.into(), stamp.clone()))
                .cloned()
        }) {
            version = Some(cached);
        }
        Some(Self {
            path: path.into(),
            stamp,
            version,
        })
    }

    pub fn same_installation(&self, other: &Self) -> bool {
        self.path == other.path && self.stamp == other.stamp
    }

    pub fn remember_version(&self, version: &str) {
        if let Ok(mut cache) = VERSIONS.get_or_init(Default::default).lock() {
            if cache.len() >= 64 {
                cache.clear();
            }
            cache.insert((self.path.clone(), self.stamp.clone()), version.into());
        }
    }
}
