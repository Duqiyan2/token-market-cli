//! Shared npm runtime helpers; installation policy lives in policy.rs.
use crate::ToolId;
use std::path::Path;

pub(super) fn supports(tool: ToolId) -> bool {
    matches!(
        tool,
        ToolId::ClaudeCode
            | ToolId::Codex
            | ToolId::OpenClaw
            | ToolId::OpenCode
            | ToolId::Pi
            | ToolId::KimiCode
    )
}

pub(super) fn runtime_path(node: &Path) -> String {
    let mut paths = vec![node.parent().unwrap_or(node).to_path_buf()];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    std::env::join_paths(paths)
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
