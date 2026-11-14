use crate::grid::ComputeError;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct ComputePool {
    permits: Arc<Semaphore>,
}
impl ComputePool {
    pub fn new(parallelism: usize) -> Result<Self, ComputeError> {
        if parallelism == 0 || parallelism > 32 {
            return Err(ComputeError::InvalidInput(
                "compute parallelism must be 1..=32".into(),
            ));
        }
        Ok(Self {
            permits: Arc::new(Semaphore::new(parallelism)),
        })
    }
    /// Rejects excess work immediately; holds the permit inside the blocking closure
    /// even when its caller is cancelled. CPU tasks cannot escape the pool bound.
    pub async fn run<F, T>(&self, work: F) -> Result<T, ComputeError>
    where
        F: FnOnce() -> Result<T, ComputeError> + Send + 'static,
        T: Send + 'static,
    {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| ComputeError::Capacity)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        })
        .await
        .map_err(|error| ComputeError::Worker(error.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cpu_work_runs_off_async_executor_and_capacity_is_bounded() {
        let pool = ComputePool::new(1).unwrap();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = pool.clone();
        let handle = tokio::spawn(async move {
            worker
                .run(move || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(7)
                })
                .await
        });
        let entered = tokio::time::timeout(std::time::Duration::from_secs(2), entered_rx).await;
        assert!(entered.is_ok() && entered.unwrap().is_ok());
        assert_eq!(pool.run(|| Ok(9)).await, Err(ComputeError::Capacity));
        release_tx.send(()).unwrap();
        assert_eq!(handle.await.unwrap().unwrap(), 7);
        assert_eq!(pool.run(|| Ok(9)).await.unwrap(), 9);
    }

    #[tokio::test]
    async fn cancelled_caller_does_not_release_active_cpu_permit() {
        let pool = ComputePool::new(1).unwrap();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (completed_tx, completed_rx) = tokio::sync::oneshot::channel();
        let worker = pool.clone();
        let handle = tokio::spawn(async move {
            worker
                .run(move || {
                    let _ = entered_tx.send(());
                    let _ = release_rx.recv();
                    let _ = completed_tx.send(());
                    Ok(())
                })
                .await
        });
        entered_rx.await.unwrap();
        handle.abort();
        assert_eq!(pool.run(|| Ok(9)).await, Err(ComputeError::Capacity));
        release_tx.send(()).unwrap();
        completed_rx.await.unwrap();
        let _ = handle.await;
    }
}
