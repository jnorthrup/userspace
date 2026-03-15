//! # Userspace
//! 
//! A comprehensive userspace kernel emulation library providing:
//! - Structured concurrency patterns (Kotlin coroutines style)
//! - io_uring userspace implementation
//! - eBPF JIT compilation and execution
//! - Network protocol abstractions and adapters
//! - Tensor operations with MLIR coordination

// Core modules
pub mod concurrency;
#[cfg(unix)]
pub mod handle;
pub mod dsel;

// Feature-gated modules
#[cfg(feature = "kernel")]
pub mod kernel;

#[cfg(feature = "network")]
pub mod network;

#[cfg(feature = "tensor")]
pub mod tensor;

#[cfg(feature = "database")]
pub mod database;

// Re-export commonly used types from concurrency
pub use concurrency::{
    CoroutineResult, CancellationException, Suspendable,
    CoroutineScope, CoroutineContext, StandardCoroutineScope,
    Job, JobImpl, SupervisorJobImpl,
    Deferred, DeferredImpl,
    launch, async_coroutine, run_blocking,
    Dispatchers, DispatcherEnum, CoroutineDispatcher,
    CancellationToken,
};


// Optional DSEL scaffolding
#[cfg(feature = "dsel")]
pub use crate::dsel::dsel_impl;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;
    
    #[tokio::test]
    async fn test_basic_launch() {
        timeout(Duration::from_secs(2), async {
            let job = Arc::new(JobImpl::new());
            let dispatcher = Dispatchers::default();
            let context = CoroutineContext::new(job, dispatcher);
            let scope = StandardCoroutineScope::new(context);
            
            let launched_job = launch(&scope, || async {
                tokio::time::sleep(Duration::from_millis(10)).await;
            });
            
            launched_job.join().await;
            assert!(launched_job.is_completed());
        })
        .await
        .expect("test_basic_launch timed out");
    }
    
    #[tokio::test]
    async fn test_async_coroutine() {
        timeout(Duration::from_secs(2), async {
            let job = Arc::new(JobImpl::new());
            let dispatcher = Dispatchers::default();
            let context = CoroutineContext::new(job, dispatcher);
            let scope = StandardCoroutineScope::new(context);
            
            let deferred = async_coroutine(&scope, || async {
                42
            });
            
            let result = deferred.await_result().await.unwrap();
            assert_eq!(result, 42);
        })
        .await
        .expect("test_async_coroutine timed out");
    }
}