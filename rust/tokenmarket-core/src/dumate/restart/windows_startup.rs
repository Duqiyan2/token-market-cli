//! Process startup confirmation, independent of account/task/credential state.
use anyhow::{bail, Result};
use std::{future::Future, time::Duration};

pub(super) struct Snapshot {
    pub host: bool,
    pub backend: bool,
    pub launch_error: Option<String>,
}

pub(super) async fn confirm<F, S>(expect_backend: bool, mut snapshot: S) -> Result<()>
where
    S: FnMut() -> F,
    F: Future<Output = Result<Snapshot>>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut stable_since = None;
    loop {
        let state = snapshot().await?;
        if let Some(error) = state.launch_error {
            bail!("搭子启动进程提前退出，未确认重启成功：{error}");
        }
        if state.host && (!expect_backend || state.backend) {
            let since = stable_since.get_or_insert_with(tokio::time::Instant::now);
            if since.elapsed() >= Duration::from_secs(2) {
                return Ok(());
            }
        } else {
            stable_since = None;
        }
        if tokio::time::Instant::now() >= deadline {
            if state.host && expect_backend && !state.backend {
                bail!("搭子主程序已打开，但 Backend 后台未能稳定启动，未确认重启成功。请查看搭子的服务异常提示");
            }
            bail!("未能确认搭子新实例稳定启动，请手动打开；已保存的配置不受影响");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn an_open_window_without_the_previous_backend_is_not_a_successful_restart() {
        let result = confirm(true, || async {
            Ok(Snapshot {
                host: true,
                backend: false,
                launch_error: None,
            })
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("Backend"));
    }
    #[tokio::test(start_paused = true)]
    async fn a_backend_that_exits_during_startup_is_not_a_successful_restart() {
        let mut calls = 0;
        let result = confirm(true, || {
            calls += 1;
            let backend = calls < 3;
            async move {
                Ok(Snapshot {
                    host: true,
                    backend,
                    launch_error: None,
                })
            }
        })
        .await;
        assert!(result.is_err());
    }
    #[tokio::test(start_paused = true)]
    async fn delayed_backend_startup_is_confirmed_after_a_stable_interval() {
        let mut calls = 0;
        confirm(true, || {
            calls += 1;
            let backend = calls >= 3;
            async move {
                Ok(Snapshot {
                    host: true,
                    backend,
                    launch_error: None,
                })
            }
        })
        .await
        .unwrap();
        assert!(calls >= 7);
    }
    #[tokio::test(start_paused = true)]
    async fn opening_a_previously_stopped_app_does_not_require_a_logged_in_backend() {
        confirm(false, || async {
            Ok(Snapshot {
                host: true,
                backend: false,
                launch_error: None,
            })
        })
        .await
        .unwrap();
    }
}
