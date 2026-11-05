//! npm engines.node checks at preview and execution; unknown ranges are advisory.
use super::{upgrades, InstallationManager};
use crate::{inventory::HostPlatform, ToolId};
use std::path::PathBuf;

type Version = [u64; 3];
fn core(s: &str) -> Option<(Version, usize)> {
    let mut v = [0; 3];
    let mut count = 0;
    for (i, p) in s.trim_start_matches('v').split('.').enumerate() {
        if i >= 3 {
            return None;
        }
        if matches!(p, "*" | "x" | "X") {
            break;
        }
        v[i] = p.split('-').next()?.parse().ok()?;
        count += 1;
    }
    Some((v, count))
}
pub(super) fn satisfies(range: &str, actual: &str) -> Option<bool> {
    let (actual, _) = core(actual)?;
    let mut unknown = false;
    for set in range.split("||") {
        let set = set.trim();
        if set.is_empty() || set == "*" {
            return Some(true);
        }
        if let Some((a, b)) = set.split_once(" - ") {
            let (lo, _) = core(a)?;
            let (mut hi, n) = core(b)?;
            let admits = if n == 3 {
                actual >= lo && actual <= hi
            } else {
                let index = n.saturating_sub(1);
                hi[index] = hi[index].checked_add(1)?;
                actual >= lo && actual < hi
            };
            if admits {
                return Some(true);
            }
            continue;
        }
        let words = set.split_whitespace().collect::<Vec<_>>();
        let mut i = 0;
        let mut admits = true;
        while i < words.len() {
            let mut word = words[i].to_string();
            if matches!(word.as_str(), ">=" | "<=" | ">" | "<" | "^" | "~" | "=") {
                i += 1;
                word.push_str(words.get(i)?);
            }
            let (op, value) = [">=", "<=", ">", "<", "^", "~", "="]
                .iter()
                .find_map(|p| word.strip_prefix(p).map(|v| (*p, v)))
                .unwrap_or(("", &word));
            let Some((target, n)) = core(value) else {
                unknown = true;
                admits = false;
                break;
            };
            let yes = match op {
                ">=" => actual >= target,
                ">" => actual > target,
                "<=" => actual <= target,
                "<" => actual < target,
                "^" | "~" | "" | "=" => {
                    if n == 0 {
                        true
                    } else if n == 3 && matches!(op, "" | "=") {
                        actual == target
                    } else {
                        let mut hi = target;
                        let idx = if op == "^" {
                            if n < 3 {
                                n - 1
                            } else if target[0] > 0 {
                                0
                            } else if target[1] > 0 {
                                1
                            } else {
                                2
                            }
                        } else if op == "~" {
                            if n >= 2 {
                                1
                            } else {
                                0
                            }
                        } else {
                            n - 1
                        };
                        hi[idx] = hi[idx].checked_add(1)?;
                        for item in hi.iter_mut().skip(idx + 1) {
                            *item = 0;
                        }
                        actual >= target && actual < hi
                    }
                }
                _ => {
                    unknown = true;
                    false
                }
            };
            admits &= yes;
            i += 1;
        }
        if admits {
            return Some(true);
        }
    }
    if unknown {
        None
    } else {
        Some(false)
    }
}
impl InstallationManager {
    fn upgrade_node(&self, tool: ToolId) -> Option<PathBuf> {
        if tool == ToolId::MiniMaxCode {
            return None;
        }
        let entry = self.environment.command_path(tool.command())?;
        let real = entry.canonicalize().ok()?;
        if !real.components().any(|c| c.as_os_str() == "node_modules")
            || real
                .components()
                .any(|c| matches!(c.as_os_str().to_str(), Some("Cellar" | "Caskroom" | ".bun")))
        {
            return None;
        }
        let sibling = entry
            .parent()?
            .join(if self.environment.platform == HostPlatform::Windows {
                "node.exe"
            } else {
                "node"
            });
        if sibling.is_file() {
            Some(sibling)
        } else {
            self.environment.command_path("node")
        }
    }
    pub(super) fn ensure_node_requirement(&self, tool: ToolId) -> anyhow::Result<()> {
        let range = self
            .node_requirements
            .lock()
            .unwrap()
            .get(&tool.to_string())
            .cloned();
        if let (Some(range), Some(node)) = (range, self.upgrade_node(tool)) {
            if let Ok(actual) = self.executor.runtime_version(&node, &node) {
                if satisfies(&range, &actual) == Some(false) {
                    anyhow::bail!(
                        "新版本要求 Node {range}，当前安装使用 {actual}；请先升级 Node.js"
                    )
                }
            }
        }
        Ok(())
    }
    pub(super) async fn load_node_requirement(
        &self,
        client: &reqwest::Client,
        tool: ToolId,
        version: &str,
    ) {
        self.node_requirements
            .lock()
            .unwrap()
            .remove(&tool.to_string());
        if self.upgrade_node(tool).is_none() {
            return;
        }
        let Some(package) = upgrades::package(tool) else {
            return;
        };
        let url = format!(
            "https://registry.npmjs.org/{}/{}",
            package.replace('/', "%2f"),
            version
        );
        if let Ok(body) = super::updates::fetch_body(client, &url).await {
            if let Some(range) = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|j| j["engines"]["node"].as_str().map(str::to_owned))
            {
                self.node_requirements
                    .lock()
                    .unwrap()
                    .insert(tool.to_string(), range);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn npm_runtime_ranges() {
        for (range, version, expected) in [
            (">=24.16.0 <25 || >=26.1.0", "22.23.2", false),
            (">=24.16.0 <25 || >=26.1.0", "24.16.0", true),
            ("^22.19.0", "22.20.1", true),
            ("^22.19.0", "24.0.0", false),
            ("~20.2", "20.3.0", false),
            ("20.x", "20.8.0", true),
            ("20 - 24", "24.9.1", true),
        ] {
            assert_eq!(satisfies(range, version), Some(expected));
        }
        assert_eq!(satisfies("unknown", "24.0.0"), None);
    }
}

#[cfg(all(test, unix))]
mod execution_tests {
    use super::*;
    use crate::installation::{InstallExecutor, InstallStep};
    use crate::inventory::InstallationEnvironment;
    use std::{
        path::Path,
        sync::{atomic::AtomicBool, Arc, Mutex},
    };
    struct NodeFixture(Mutex<String>);
    impl InstallExecutor for NodeFixture {
        fn execute(
            &self,
            _: &InstallStep,
            _: &AtomicBool,
            _: &mut dyn FnMut(String),
        ) -> anyhow::Result<()> {
            panic!("blocked runtime must not execute an installer")
        }
        fn version(&self, _: &Path) -> anyhow::Result<String> {
            Ok("1.0.0".into())
        }
        fn runtime_version(&self, _: &Path, _: &Path) -> anyhow::Result<String> {
            Ok(self.0.lock().unwrap().clone())
        }
    }
    #[test]
    fn a_changed_node_runtime_is_rechecked_after_upgrade_confirmation() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let package = home.join("lib/node_modules/@openai/codex");
        std::fs::create_dir_all(package.join("bin")).unwrap();
        std::fs::write(package.join("package.json"), r#"{"name":"@openai/codex"}"#).unwrap();
        std::fs::write(package.join("bin/cli.js"), "fixture").unwrap();
        std::fs::set_permissions(
            package.join("bin/cli.js"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        symlink(package.join("bin/cli.js"), bin.join("codex")).unwrap();
        for name in ["node", "npm"] {
            std::fs::write(bin.join(name), "fixture").unwrap();
            std::fs::set_permissions(bin.join(name), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let env = InstallationEnvironment {
            home: home.into(),
            platform: HostPlatform::MacOs,
            command_directories: vec![bin],
            application_directories: vec![],
            executable_extensions: vec![],
        };
        let exec = Arc::new(NodeFixture(Mutex::new("v20.0.0".into())));
        let m = InstallationManager::new(env, "aarch64".into(), exec.clone());
        m.node_requirements
            .lock()
            .unwrap()
            .insert("codex".into(), ">=22".into());
        let u = m.inspect_update(ToolId::Codex, Ok("2.0.0".into()));
        assert_eq!(u.state, "node_required");
        assert!(!u.can_upgrade);
        *exec.0.lock().unwrap() = "v24.0.0".into();
        let u = m.inspect_update(ToolId::Codex, Ok("2.0.0".into()));
        assert!(u.can_upgrade);
        *exec.0.lock().unwrap() = "v20.0.0".into();
        assert!(m
            .start_upgrade_confirmed(
                ToolId::Codex,
                "2.0.0",
                u.upgrade_revision.as_deref().unwrap()
            )
            .unwrap_err()
            .to_string()
            .contains("Node"));
    }
}
