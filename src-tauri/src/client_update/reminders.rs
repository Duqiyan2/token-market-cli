use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Default, Deserialize, Serialize)]
struct Preferences {
    ignored_version: Option<String>,
}

pub(super) struct Reminders {
    path: Option<PathBuf>,
    preferences: Preferences,
    unreadable: bool,
    deferred: bool,
}

impl Reminders {
    pub fn new(path: Option<PathBuf>) -> Self {
        let loaded = match path.as_ref().map(fs::read) {
            Some(Ok(bytes)) => serde_json::from_slice(&bytes).map_err(|_| ()),
            None => Ok(Preferences::default()),
            Some(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Preferences::default())
            }
            Some(Err(_)) => Err(()),
        };
        let unreadable = loaded.is_err();
        Self {
            path,
            preferences: loaded.unwrap_or_default(),
            unreadable,
            deferred: false,
        }
    }

    pub fn later(&mut self) {
        self.deferred = true;
    }

    pub fn ignore(&mut self, version: &str) -> Result<(), String> {
        let preferences = Preferences {
            ignored_version: Some(version.into()),
        };
        if let Some(path) = &self.path {
            let temporary = path.with_extension("json.tmp");
            tokenmarket_core::config::write_private_json(&temporary, &preferences)
                .and_then(|()| fs::rename(&temporary, path).map_err(Into::into))
                .map_err(|error| format!("无法保存忽略版本，请重试：{error}"))?;
        }
        self.preferences = preferences;
        self.unreadable = false;
        Ok(())
    }

    pub fn eligible(&self, version: &str) -> bool {
        !self.deferred
            && !self.unreadable
            && self.preferences.ignored_version.as_deref() != Some(version)
    }
}
