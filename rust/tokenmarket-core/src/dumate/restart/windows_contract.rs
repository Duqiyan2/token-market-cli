//! Pure validation shared by Windows discovery and host-side contract tests.
use anyhow::{bail, Context, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub(super) fn same_path(left: &str, right: &str) -> bool {
    fn normalized(path: &str) -> String {
        path.strip_prefix(r"\\?\")
            .unwrap_or(path)
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }
    normalized(left) == normalized(right)
}
pub(super) fn within(path: &str, directory: &str) -> bool {
    let path = path
        .strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('/', "\\")
        .to_lowercase();
    let directory = directory
        .strip_prefix(r"\\?\")
        .unwrap_or(directory)
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    path.starts_with(&(directory + "\\"))
}

/// Capture order is arbitrary. These indices identify the batch whose exit
/// must be confirmed before any remaining service is shut down.
pub(super) fn shutdown_hosts(processes: &[(u32, u32, bool)]) -> Vec<usize> {
    processes
        .iter()
        .enumerate()
        .filter_map(|(index, (_, parent, is_app))| {
            (*is_app
                && !processes
                    .iter()
                    .any(|(pid, _, is_parent_app)| *pid == *parent && *is_parent_app))
            .then_some(index)
        })
        .collect()
}
pub(super) fn validate_app(app: &Path) -> Result<()> {
    if !app
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("DuMate.exe"))
    {
        bail!("搭子程序名称无法确认")
    }
    // Read only package identity, never unpack or modify the installed app.
    let mut file = File::open(
        app.parent()
            .context("搭子目录无效")?
            .join("resources/app.asar"),
    )?;
    let mut first = [0u8; 8];
    file.read_exact(&mut first)?;
    let size = u32::from_le_bytes(first[4..8].try_into()?) as usize;
    if !(8..=16 * 1024 * 1024).contains(&size) {
        bail!("搭子安装包格式无法确认")
    }
    let mut header = vec![0; size];
    file.read_exact(&mut header)?;
    let length = u32::from_le_bytes(header[4..8].try_into()?) as usize;
    let index: serde_json::Value = serde_json::from_slice(
        header
            .get(8..8usize.checked_add(length).context("搭子索引无效")?)
            .context("搭子索引无效")?,
    )?;
    let entry = &index["files"]["package.json"];
    if entry["unpacked"] == true || !entry["link"].is_null() {
        bail!("搭子包身份无法确认")
    }
    let offset = entry["offset"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .context("搭子包身份无效")?;
    let length = entry["size"]
        .as_u64()
        .filter(|n| *n <= 65536)
        .context("搭子包身份无效")?;
    let offset = (8 + size as u64)
        .checked_add(offset)
        .context("搭子包身份无效")?;
    if offset
        .checked_add(length)
        .is_none_or(|end| end > file.metadata().map(|m| m.len()).unwrap_or(0))
    {
        bail!("搭子包身份不完整")
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; length as usize];
    file.read_exact(&mut bytes)?;
    let package: serde_json::Value = serde_json::from_slice(&bytes)?;
    if package["name"] != "qianfan-desktop-app" {
        bail!("无法确认百度搭子应用身份，请手动重启搭子")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restarting_does_not_leave_a_live_host_reporting_backend_exit() {
        // Snapshot order: backend, renderer, Electron main process.
        let processes = [(30, 10, false), (20, 10, true), (10, 1, true)];
        let hosts = shutdown_hosts(&processes);
        let mut host_alive = true;
        let mut backend_error = false;
        // A Windows shutdown batch does not promise main-before-service order.
        for index in hosts
            .iter()
            .copied()
            .chain((0..3).filter(|i| !hosts.contains(i)))
        {
            if index == 0 && host_alive {
                backend_error = true;
            }
            if index == 2 {
                host_alive = false;
            }
        }
        assert!(
            !backend_error,
            "Backend exited while the previous DuMate host was still alive (0x10000001)"
        );
    }

    #[test]
    fn host_shutdown_handles_multiple_roots_and_background_only_instances() {
        assert_eq!(
            shutdown_hosts(&[
                (20, 10, true),
                (10, 1, true),
                (40, 2, true),
                (30, 10, false)
            ]),
            [1, 2]
        );
        assert!(shutdown_hosts(&[(30, 10, false)]).is_empty());
        assert!(shutdown_hosts(&[]).is_empty());
    }
    #[test]
    fn windows_package_and_process_paths_cannot_expand_to_other_installations() {
        assert!(same_path(
            r"\\?\C:\Apps\DuMate\DuMate.exe",
            "c:/apps/dumate/dumate.exe"
        ));
        assert!(within(
            r"\\?\C:\Apps\DuMate\resources\engine.exe",
            r"c:\apps\dumate\resources"
        ));
        assert!(!within(
            r"C:\Apps\DuMate\resources-other\engine.exe",
            r"C:\Apps\DuMate\resources"
        ));
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("DuMate.exe");
        std::fs::create_dir(root.path().join("resources")).unwrap();
        let write = |name: &str, version: &str, build: &str| {
            let package = serde_json::to_vec(
                &serde_json::json!({"name":name,"version":version,"buildVersion":build}),
            )
            .unwrap();
            let index = serde_json::to_vec(
                &serde_json::json!({"files":{"package.json":{"size":package.len(),"offset":"0"}}}),
            )
            .unwrap();
            let mut header = vec![0u8; 8];
            header[4..8].copy_from_slice(&(index.len() as u32).to_le_bytes());
            header.extend(index);
            let mut bytes = vec![0u8; 8];
            bytes[4..8].copy_from_slice(&(header.len() as u32).to_le_bytes());
            bytes.extend(header);
            bytes.extend(package);
            std::fs::write(root.path().join("resources/app.asar"), bytes).unwrap();
        };
        write("qianfan-desktop-app", "1.0.89", "447");
        validate_app(&app).unwrap();
        for version in ["1.0.89.0", "1.0.88", "1.0.90", "2.0.0"] {
            write("qianfan-desktop-app", version, "500");
            validate_app(&app).unwrap();
        }
        write("another-app", "1.0.89", "447");
        assert!(validate_app(&app).is_err());
        std::fs::write(root.path().join("resources/app.asar"), [0xff; 8]).unwrap();
        assert!(validate_app(&app).is_err());
    }
}
