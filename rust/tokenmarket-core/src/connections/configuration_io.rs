//! File receipts use logical BYOK row snapshots for QwenWork, never database copies.
use anyhow::{bail, Result};
use std::{fs, path::Path};

pub(super) fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("配置路径是符号链接，请先确认实际路径")
    }
    if crate::qwenwork::is_database(path) {
        return crate::qwenwork::read_snapshot(path).map(Some);
    }
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => bail!("无法读取工具配置"),
    }
}

pub(super) fn write(
    path: &Path,
    expected: &Option<Vec<u8>>,
    after: &Option<Vec<u8>>,
) -> Result<()> {
    if crate::qwenwork::is_database(path) {
        return crate::qwenwork::write_snapshot(
            path,
            expected
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("千问办公数据库缺少恢复快照"))?,
            after
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("拒绝删除千问办公数据库"))?,
        );
    }
    if read(path)? != *expected {
        bail!("配置已改变，请重试")
    }
    if let Some(bytes) = after {
        super::atomic_write(path, bytes)
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
