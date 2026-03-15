use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

// Coroutine Scope abstraction for testing
struct CoroutineScope {
    active_jobs: Arc<AtomicUsize>,
    cancelled: Arc<Mutex<bool>>,
}

impl CoroutineScope {
    fn new() -> Self {
        CoroutineScope {
            active_jobs: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::new(Mutex::new(false)),
        }
    }

    async fn launch<F, T>(&self, task: F) -> Option<T>
    where
        F: std::future::Future<Output = T>,
    {
        // Check if scope is cancelled before launching
        {
            let is_cancelled = self.cancelled.lock().await;
            if *is_cancelled {
                return None;
            }
        }

        // Increment active jobs
        self.active_jobs.fetch_add(1, Ordering::SeqCst);

        // Run the task
        let result = task.await;

        // Decrement active jobs
        self.active_jobs.fetch_sub(1, Ordering::SeqCst);

        Some(result)
    }

    async fn cancel(&self) {
        let mut cancelled = self.cancelled.lock().await;
        *cancelled = true;
    }

    fn active_job_count(&self) -> usize {
        self.active_jobs.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn test_coroutine_scope_basic_launch() {
    let scope = CoroutineScope::new();

    let result = scope.launch(async {
        42
    }).await;

    assert_eq!(result, Some(42));
    assert_eq!(scope.active_job_count(), 0);
}

#[tokio::test]
async fn test_coroutine_scope_multiple_launches() {
    let scope = CoroutineScope::new();

    let results = tokio::join!(
        scope.launch(async { 1 }),
        scope.launch(async { 2 }),
        scope.launch(async { 3 })
    );

    assert_eq!(results.0, Some(1));
    assert_eq!(results.1, Some(2));
    assert_eq!(results.2, Some(3));
    assert_eq!(scope.active_job_count(), 0);
}

#[tokio::test]
async fn test_coroutine_scope_cancellation() {
    let scope = CoroutineScope::new();

    // Launch a long-running task
    let long_task = scope.launch(async {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        "Task completed"
    });

    // Cancel the scope immediately
    scope.cancel().await;

    // Verify task is not launched
    let result = long_task.await;
    assert_eq!(result, None);
    assert_eq!(scope.active_job_count(), 0);
}

#[tokio::test]
async fn test_coroutine_scope_concurrent_operations() {
    let scope = CoroutineScope::new();
    let shared_counter = Arc::new(AtomicUsize::new(0));

    // Concurrent increment operations
    let increments = tokio::join!(
        scope.launch({
            let counter = Arc::clone(&shared_counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        }),
        scope.launch({
            let counter = Arc::clone(&shared_counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        }),
        scope.launch({
            let counter = Arc::clone(&shared_counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        })
    );

    // All tasks should complete
    assert!(increments.0.is_some());
    assert!(increments.1.is_some());
    assert!(increments.2.is_some());

    // Counter should be incremented 3 times
    assert_eq!(shared_counter.load(Ordering::SeqCst), 3);
    assert_eq!(scope.active_job_count(), 0);
}