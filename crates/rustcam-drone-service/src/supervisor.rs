//! 后台任务的 panic 隔离（规则 §16 风险表：`tokio::spawn` panic 会拖垮进程）。
//!
//! 受监管的任务体 panic 时只记录日志，不会让进程退出，也不会影响其它后台任务。

use std::future::Future;
use std::panic::AssertUnwindSafe;

use futures::FutureExt;

/// 运行一个受监管的后台任务。
pub fn spawn_supervised<F>(name: &'static str, fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        guard(name, fut).await;
    });
}

/// 受监管地运行一个 future：捕获 panic 并记录，返回时任务体已结束。
pub async fn guard<F>(name: &'static str, fut: F)
where
    F: Future<Output = ()>,
{
    if let Err(payload) = AssertUnwindSafe(fut).catch_unwind().await {
        tracing::error!(
            task = name,
            detail = %panic_detail(&payload),
            "后台任务 panic，已终止"
        );
    }
}

fn panic_detail(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        (*msg).to_string()
    } else if let Some(msg) = payload.downcast_ref::<String>() {
        msg.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn guard_survives_panic() {
        // 任务体 panic 不应向外传播。
        guard("boom", async { panic!("boom") }).await;
    }

    #[tokio::test]
    async fn guard_runs_normal_task() {
        let (tx, rx) = oneshot::channel();
        guard("ok", async move {
            let _ = tx.send(7);
        })
        .await;
        assert_eq!(rx.await.unwrap(), 7);
    }

    #[tokio::test]
    async fn spawned_task_panic_does_not_kill_runtime() {
        spawn_supervised("boom", async { panic!("boom") });
        // 让被监管的任务跑完 panic。
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (tx, rx) = oneshot::channel();
        spawn_supervised("alive", async move {
            let _ = tx.send(1);
        });
        assert_eq!(rx.await.unwrap(), 1, "runtime 仍在运行");
    }
}
