//! Read-only device tool usage; device identity and management token stay native.
use crate::desktop::DesktopSession;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageQuery {
    pub owner_id: String,
    pub workspace_id: String,
    pub start_time: i64,
    pub end_time: i64,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub status: String,
    pub cursor: Option<String>,
    pub page: u32,
    pub page_size: u32,
}
impl UsageQuery {
    pub fn validate(&self, session: &DesktopSession) -> Result<()> {
        if self.owner_id != session.user.id || self.workspace_id != session.default_workspace.id {
            bail!("账号或当前空间已改变，请刷新用量");
        }
        if !session.is_valid() {
            bail!("登录已过期，请重新登录");
        }
        if self.workspace_id.is_empty()
            || !self.workspace_id.bytes().all(|c| c.is_ascii_digit())
            || self.start_time <= 0
            || self.end_time < self.start_time
            || self.end_time - self.start_time > 31 * 86_400_000
            || !(1..=100).contains(&self.page_size)
            || !(1..=10_000).contains(&self.page)
        {
            bail!("用量查询范围或分页参数无效");
        }
        if !self.tool.is_empty() {
            crate::tool_id(&self.tool)?;
        }
        if self.model.len() > 128
            || !["", "ALL", "SUCCESS", "FAILED", "CANCELLED"].contains(&self.status.as_str())
            || self.cursor.as_ref().is_some_and(|value| value.len() > 256)
        {
            bail!("用量筛选条件无效");
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub enum UsageKind {
    Logs,
    Buckets,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    pub tools: Vec<UsageTool>,
    pub buckets: Vec<UsageBucket>,
    pub items: Vec<UsageRow>,
    pub next_cursor: Option<String>,
    pub page: u32,
    pub page_size: u32,
    pub as_of: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTool {
    pub id: String,
    pub name: String,
    pub key_count: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageBucket {
    pub date: String,
    pub request_count: u64,
    pub cost_micro: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub input_token_count: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub request_id: String,
    pub key_id: Option<String>,
    pub key_name: Option<String>,
    pub tool: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub status_code: Option<i32>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_micro: String,
    pub duration_ms: Option<i64>,
    pub ttft_ms: Option<i64>,
    pub started_at: i64,
}
pub struct UsageClient {
    base_url: String,
    http: reqwest::Client,
}
impl UsageClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("usage HTTP client"),
        }
    }
    pub async fn query(
        &self,
        session: &DesktopSession,
        device_id: &str,
        query: &UsageQuery,
        kind: UsageKind,
    ) -> Result<UsageResponse> {
        query.validate(session)?;
        if !(8..=128).contains(&device_id.len())
            || !device_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            bail!("本机设备标识无效");
        }
        let kind = match kind {
            UsageKind::Logs => "logs",
            UsageKind::Buckets => "buckets",
        };
        let response = self
            .http
            .post(format!(
                "{}/api/client/usage/{kind}",
                self.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&session.session_token)
            .header("X-Client-Owner-Id", &session.user.id)
            .header("X-Workspace-Id", &query.workspace_id)
            .timeout(std::time::Duration::from_secs(30))
            .json(&serde_json::json!({
                "workspaceId": query.workspace_id, "deviceId": device_id,
                "startTime": query.start_time, "endTime": query.end_time,
                "tool": query.tool, "model": query.model, "status": query.status,
                "cursor": query.cursor, "page": query.page, "pageSize": query.page_size
            }))
            .send()
            .await
            .context("无法连接用量服务，请稍后重试")?;
        let status = response.status();
        if matches!(status.as_u16(), 401 | 403) {
            bail!("登录状态或空间权限已改变，请刷新后重试");
        }
        if status.as_u16() == 404 {
            bail!("服务端暂未启用桌面用量查询，请稍后重试");
        }
        let payload: serde_json::Value = response.json().await.context("用量服务响应无效")?;
        if !status.is_success() || payload["success"].as_bool() != Some(true) {
            bail!("无法读取用量，请刷新并检查当前空间的日志查看权限");
        }
        serde_json::from_value(payload["data"].clone()).context("用量服务响应缺少必要字段")
    }
}
