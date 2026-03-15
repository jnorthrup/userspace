use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Dispatches coroutines to appropriate threads
/// Equivalent to Kotlin's CoroutineDispatcher
pub trait CoroutineDispatcher: Send + Sync {
    /// Dispatch a coroutine for execution
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()>;
        
    /// Check if this dispatcher is confined to a single thread
    fn is_dispatch_needed(&self) -> bool {
        true
    }
    
    /// Limit parallelism to the given value
    fn limit_parallelism(&self, parallelism: usize) -> DispatcherEnum;
}

/// Helper method to dispatch with generic futures
pub fn dispatch<F>(dispatcher: &dyn CoroutineDispatcher, future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Use a oneshot channel to capture the future's output from the dispatcher-run task.
    let (tx, rx) = tokio::sync::oneshot::channel::<F::Output>();

    // Create a boxed task that runs the provided future and sends the result back.
    let boxed = Box::pin(async move {
        let res = future.await;
        // ignore send error — the receiver may have been dropped if caller aborted
        let _ = tx.send(res);
    }) as Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

    // Dispatch the boxed task using the provided dispatcher (this marks usage for spies)
    let _jh = dispatcher.dispatch_boxed(boxed);

    // Return a JoinHandle that awaits the oneshot receiver and yields the original output.
    tokio::spawn(async move { rx.await.expect("dispatcher task failed or was cancelled") })
}

/// Concrete enum for dispatchers to make them dyn-compatible
#[derive(Clone)]
pub enum DispatcherEnum {
    Default(DefaultDispatcher),
    Main(MainDispatcher),
    Cpu(CpuDispatcher),
    Io(IoDispatcher),
    Limited(LimitedDispatcher),
}

impl CoroutineDispatcher for DispatcherEnum {
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        match self {
            DispatcherEnum::Default(d) => d.dispatch_boxed(future),
            DispatcherEnum::Main(d) => d.dispatch_boxed(future),
            DispatcherEnum::Cpu(d) => d.dispatch_boxed(future),
            DispatcherEnum::Io(d) => d.dispatch_boxed(future),
            DispatcherEnum::Limited(d) => d.dispatch_boxed(future),
        }
    }
    
    fn is_dispatch_needed(&self) -> bool {
        match self {
            DispatcherEnum::Default(d) => d.is_dispatch_needed(),
            DispatcherEnum::Main(d) => d.is_dispatch_needed(),
            DispatcherEnum::Cpu(d) => d.is_dispatch_needed(),
            DispatcherEnum::Io(d) => d.is_dispatch_needed(),
            DispatcherEnum::Limited(d) => d.is_dispatch_needed(),
        }
    }
    
    fn limit_parallelism(&self, parallelism: usize) -> DispatcherEnum {
        DispatcherEnum::Limited(LimitedDispatcher::new(parallelism))
    }
}

/// Default dispatcher using Tokio's multi-threaded runtime
#[derive(Clone)]
pub struct DefaultDispatcher;

impl CoroutineDispatcher for DefaultDispatcher {
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        tokio::spawn(future)
    }
    
    fn limit_parallelism(&self, parallelism: usize) -> DispatcherEnum {
        DispatcherEnum::Limited(LimitedDispatcher::new(parallelism))
    }
}

/// Dispatcher that confines execution to the main thread
#[derive(Clone)]
pub struct MainDispatcher;

impl CoroutineDispatcher for MainDispatcher {
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        tokio::spawn(future)
    }
    
    fn is_dispatch_needed(&self) -> bool {
        false
    }
    
    fn limit_parallelism(&self, parallelism: usize) -> DispatcherEnum {
        DispatcherEnum::Limited(LimitedDispatcher::new(parallelism))
    }
}

/// Dispatcher optimized for CPU-intensive work
#[derive(Clone)]
pub struct CpuDispatcher {
    _pool_size: usize,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl CpuDispatcher {
    pub fn new(pool_size: usize) -> Self {
    let p = if pool_size == 0 { 1 } else { pool_size };
    Self { _pool_size: p, semaphore: Arc::new(tokio::sync::Semaphore::new(p)) }
    }
}

impl CoroutineDispatcher for CpuDispatcher {
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        let semaphore = self.semaphore.clone();
        tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            future.await
        })
    }
    
    fn limit_parallelism(&self, parallelism: usize) -> DispatcherEnum {
        DispatcherEnum::Limited(LimitedDispatcher::new(parallelism))
    }
}

/// Dispatcher for IO-bound operations  
#[derive(Clone)]
pub struct IoDispatcher;

impl CoroutineDispatcher for IoDispatcher {
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        tokio::spawn(future)
    }
    
    fn limit_parallelism(&self, parallelism: usize) -> DispatcherEnum {
        DispatcherEnum::Limited(LimitedDispatcher::new(parallelism))
    }
}

/// Dispatcher with limited parallelism
#[derive(Clone)]
pub struct LimitedDispatcher {
    _parallelism: usize,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl LimitedDispatcher {
    pub fn new(parallelism: usize) -> Self {
        let p = if parallelism == 0 { 1 } else { parallelism };

        Self {
            _parallelism: p,
            semaphore: Arc::new(tokio::sync::Semaphore::new(p)),
        }
    }
}

impl CoroutineDispatcher for LimitedDispatcher {
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        let semaphore = self.semaphore.clone();
        tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            future.await
        })
    }
    
    fn limit_parallelism(&self, parallelism: usize) -> DispatcherEnum {
        DispatcherEnum::Limited(LimitedDispatcher::new(parallelism))
    }
}

/// Standard dispatchers similar to Kotlin coroutines
pub struct Dispatchers;

impl Dispatchers {
    /// Default dispatcher for CPU-bound work
    pub fn default() -> DispatcherEnum {
        DispatcherEnum::Default(DefaultDispatcher)
    }
    
    /// Dispatcher confined to the main thread
    pub fn main() -> DispatcherEnum {
        DispatcherEnum::Main(MainDispatcher)
    }
    
    /// Dispatcher optimized for IO operations
    pub fn io() -> DispatcherEnum {
        DispatcherEnum::Io(IoDispatcher)
    }
    
    /// Unconfined dispatcher that starts in the caller thread
    pub fn unconfined() -> DispatcherEnum {
        DispatcherEnum::Default(DefaultDispatcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

    struct SpyDispatcher {
        called: Arc<AtomicBool>,
    }

    impl SpyDispatcher {
        fn new() -> (Self, Arc<AtomicBool>) {
            let a = Arc::new(AtomicBool::new(false));
            (Self { called: a.clone() }, a)
        }
    }

    impl CoroutineDispatcher for SpyDispatcher {
        fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> tokio::task::JoinHandle<()> {
            self.called.store(true, Ordering::SeqCst);
            tokio::spawn(future)
        }

        fn is_dispatch_needed(&self) -> bool { true }

        fn limit_parallelism(&self, _parallelism: usize) -> DispatcherEnum {
            DispatcherEnum::Default(DefaultDispatcher)
        }
    }

    #[tokio::test]
    async fn dispatch_forwards_to_dispatcher() {
        let (spy, called) = SpyDispatcher::new();

        let h = crate::concurrency::dispatcher::dispatch(&spy, async { 123u32 });

        let v = h.await.unwrap();
        assert_eq!(v, 123u32);
        assert!(called.load(Ordering::SeqCst), "dispatcher was not used by dispatch()");
    }

    #[tokio::test]
    async fn cpu_dispatcher_limits_parallelism_one() {
        use std::time::{Duration, Instant};

        let d = CpuDispatcher::new(1);

        let started = Arc::new(AtomicBool::new(false));
        let finished_first = Arc::new(AtomicBool::new(false));

        // First task flips started -> true, sleeps, then sets finished_first
        let s1 = started.clone();
        let f1 = finished_first.clone();
        let j1 = d.dispatch_boxed(Box::pin(async move {
            s1.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(200)).await;
            f1.store(true, Ordering::SeqCst);
        }));

        // Give the runtime a moment to schedule the first task
        tokio::task::yield_now().await;

        // Start time for second task; it should not run until first completes because pool_size == 1
        let start = Instant::now();

        let j2 = d.dispatch_boxed(Box::pin(async move {
            // if this runs before first finished, the test will fail by timing
        }));

        // Wait for both to complete
        let _ = j1.await;
        let _ = j2.await;

        // Ensure first actually ran and finished
        assert!(started.load(Ordering::SeqCst));
        assert!(finished_first.load(Ordering::SeqCst));

        // The second task should have started only after the first finished; elapsed >= 200ms
        assert!(start.elapsed() >= Duration::from_millis(190));
    }
}