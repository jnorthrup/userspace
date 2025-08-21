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
    tokio::spawn(future)
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
    pool_size: usize,
}

impl CpuDispatcher {
    pub fn new(pool_size: usize) -> Self {
        Self { pool_size }
    }
}

impl CoroutineDispatcher for CpuDispatcher {
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        tokio::spawn(future)
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
    parallelism: usize,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl LimitedDispatcher {
    pub fn new(parallelism: usize) -> Self {
        Self {
            parallelism,
            semaphore: Arc::new(tokio::sync::Semaphore::new(parallelism)),
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