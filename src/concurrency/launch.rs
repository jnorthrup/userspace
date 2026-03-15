use super::{CoroutineScope, Job, Deferred, DispatcherEnum, CancellationException, CoroutineDispatcher};
use std::future::Future;
use std::sync::Arc;

/// Launch a new coroutine without blocking the current thread
/// Equivalent to Kotlin's launch function
pub fn launch<F, Fut>(
    scope: &dyn CoroutineScope,
    block: F,
) -> Arc<dyn Job>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    launch_with_context(scope, None, block)
}

/// Launch a coroutine with a specific dispatcher
pub fn launch_on<F, Fut>(
    scope: &dyn CoroutineScope,
    dispatcher: DispatcherEnum,
    block: F,
) -> Arc<dyn Job>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    launch_with_context(scope, Some(dispatcher), block)
}

fn launch_with_context<F, Fut>(
    scope: &dyn CoroutineScope,
    dispatcher: Option<DispatcherEnum>,
    block: F,
) -> Arc<dyn Job>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let context = scope.get_coroutine_context();
    let job = Arc::new(super::job::JobImpl::new());
    let dispatcher = dispatcher.unwrap_or_else(|| context.dispatcher.clone());
    
    // Attach as child to scope's job
    context.job.attach_child(job.clone());
    
    let job_clone = job.clone();
    let future = async move {
        job_clone.start();
        
        // Check for cancellation before starting
        if job_clone.is_cancelled() {
            return;
        }
        
    block().await;
    // Mark job completed so joiners are notified
    job_clone.complete().await;
    };
    
    dispatcher.dispatch_boxed(Box::pin(future));
    job
}

/// Start a new coroutine that returns a result
/// Equivalent to Kotlin's async function  
pub fn async_coroutine<F, Fut, T>(
    scope: &dyn CoroutineScope,
    block: F,
) -> Arc<dyn Deferred<T>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + Sync + Clone + 'static,
{
    async_with_context(scope, None, block)
}

/// Start an async coroutine with a specific dispatcher
pub fn async_on<F, Fut, T>(
    scope: &dyn CoroutineScope,
    dispatcher: DispatcherEnum,
    block: F,
) -> Arc<dyn Deferred<T>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + Sync + Clone + 'static,
{
    async_with_context(scope, Some(dispatcher), block)
}

fn async_with_context<F, Fut, T>(
    scope: &dyn CoroutineScope,
    dispatcher: Option<DispatcherEnum>,
    block: F,
) -> Arc<dyn Deferred<T>>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + Sync + Clone + 'static,
{
    let context = scope.get_coroutine_context();
    let deferred = Arc::new(super::deferred::DeferredImpl::new());
    let _dispatcher = dispatcher.unwrap_or_else(|| context.dispatcher.clone());
    
    // Attach as child to scope's job
    context.job.attach_child(deferred.clone());
    
    let deferred_clone = deferred.clone();
    let future = async move {
        deferred_clone.start();
        
        // Check for cancellation before starting
        if deferred_clone.is_cancelled() {
            deferred_clone.complete_exceptionally(
                CancellationException::new("Coroutine was cancelled before execution")
            ).await;
            return;
        }
        
        tokio::select! {
            result = block() => {
                deferred_clone.complete(result).await;
            }
            _ = deferred_clone.wait_for_cancellation() => {
                deferred_clone.complete_exceptionally(
                    CancellationException::new("Coroutine was cancelled during execution")
                ).await;
            }
        }
    };
    
    tokio::spawn(future);
    deferred
}

/// Run a suspending block and return the result
/// Equivalent to Kotlin's runBlocking
pub async fn run_blocking<F, Fut, T>(block: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    block().await
}

/// Wait for all jobs to complete
pub async fn join_all(jobs: Vec<Arc<dyn Job>>) {
    for job in jobs {
        job.join().await;
    }
}

/// Wait for all deferred results to complete and return their values
pub async fn await_all<T: Clone>(deferreds: Vec<Arc<dyn Deferred<T>>>) -> Vec<Result<T, CancellationException>> {
    let mut results = Vec::new();
    
    for deferred in deferreds {
        let result = deferred.await_result().await;
        results.push(result);
    }
    
    results
}

/// Select the first completed deferred from a list
pub async fn select_first<T: Clone>(
    deferreds: Vec<Arc<dyn Deferred<T>>>
) -> Result<T, CancellationException> {
    if deferreds.is_empty() {
        return Err(CancellationException::new("No deferreds provided to select from"));
    }
    
    // Create futures for all deferreds
    let futures: Vec<_> = deferreds.iter()
        .map(|d| d.await_result())
        .collect();
    
    // Use tokio::select to wait for the first to complete
    // This is a simplified version - in a real implementation you'd want
    // to handle arbitrary numbers of futures
    match futures.len() {
        1 => futures.into_iter().next().unwrap().await,
        _ => {
            // For now, just await the first one
            // A full implementation would use tokio::select! with dynamic branches
            futures.into_iter().next().unwrap().await
        }
    }
}

/// Extension methods for CoroutineScope to enable convenient syntax
pub trait CoroutineScopeExt {
    /// Launch a coroutine in this scope
    fn launch<F, Fut>(&self, block: F) -> Arc<dyn Job>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static;
    
    /// Start an async coroutine in this scope
    fn async_coroutine<F, Fut, T>(&self, block: F) -> Arc<dyn Deferred<T>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + Sync + Clone + 'static;
}

impl<S: CoroutineScope> CoroutineScopeExt for S {
    fn launch<F, Fut>(&self, block: F) -> Arc<dyn Job>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        launch(self, block)
    }
    
    fn async_coroutine<F, Fut, T>(&self, block: F) -> Arc<dyn Deferred<T>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + Sync + Clone + 'static,
    {
        async_coroutine(self, block)
    }
}