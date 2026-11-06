//! Waiting policy for the Windows Restart Manager worker.
use anyhow::{bail, Result};
use std::{future::Future, time::Duration};

pub(super) async fn finish(
    shutdown: impl Future<Output = Result<()>>,
    cancel: impl FnOnce(),
) -> Result<()> {
    // RmForceShutdown may wait 30 seconds before forcing an unresponsive app
    // to exit. Leave time for that OS deadline and its completion bookkeeping.
    // https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/ne-restartmanager-rm_shutdown_type
    let mut shutdown = std::pin::pin!(shutdown);
    match tokio::time::timeout(Duration::from_secs(60), &mut shutdown).await {
        Ok(result) => result,
        Err(_) => {
            cancel();
            // Cancellation requests do not join the blocking Windows worker.
            // Keep the caller's restart/configuration guard until it settles,
            // so an old shutdown cannot kill a later replacement instance.
            let _ = shutdown.await;
            bail!("搭子进程结束超时，请手动退出后重新打开");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test(start_paused = true)]
    async fn windows_force_shutdown_can_finish_at_its_default_thirty_second_deadline() {
        let cancelled = Cell::new(false);
        let result = finish(
            async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                Ok(())
            },
            || cancelled.set(true),
        )
        .await;
        assert!(
            result.is_ok(),
            "Windows had not reached its force-shutdown deadline: {result:?}"
        );
        assert!(!cancelled.get());
    }

    #[tokio::test(start_paused = true)]
    async fn timed_out_shutdown_settles_before_the_restart_guard_can_be_released() {
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        let settled = Cell::new(false);
        let result = finish(
            async {
                cancelled.await?;
                tokio::time::sleep(Duration::from_secs(1)).await;
                settled.set(true);
                Ok(())
            },
            || {
                let _ = cancel.send(());
            },
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("超时"));
        assert!(
            settled.get(),
            "shutdown must not outlive the restart/configuration guard"
        );
    }

    #[tokio::test]
    async fn a_known_windows_shutdown_error_is_returned_without_cancellation() {
        let result = finish(async { bail!("Windows 错误码 351") }, || {
            panic!("completed shutdown must not be cancelled")
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("351"));
    }
}
