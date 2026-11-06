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
pub(super) fn validate_app(app: &Path) -> Result<()> {
    if !app
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("DuMate.exe"))
    {
        bail!("搭子程序名称无法确认")
    }
    // Read only package identity/version, never unpack or modify the installed app.
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
    if package["name"] != "qianfan-desktop-app"
        || package["version"] != "1.0.89"
        || package["buildVersion"].as_str() != Some("447")
    {
        bail!("当前搭子版本尚不支持一键重启，请手动重启搭子")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let write = |version: &str| {
            let package = serde_json::to_vec(&serde_json::json!({"name":"qianfan-desktop-app","version":version,"buildVersion":"447"})).unwrap();
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
        write("1.0.89");
        validate_app(&app).unwrap();
        write("1.0.90");
        assert!(validate_app(&app).is_err());
        std::fs::write(root.path().join("resources/app.asar"), [0xff; 8]).unwrap();
        assert!(validate_app(&app).is_err());
    }
}
