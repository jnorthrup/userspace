use super::{DispatcherEnum, Job, CancellationException};
use std::sync::Arc;

/// Defines a scope for new coroutines
/// Equivalent to Kotlin's CoroutineScope
pub trait CoroutineScope: Send + Sync {
    /// The context of this scope, containing the Job and Dispatcher
    fn get_coroutine_context(&self) -> &CoroutineContext;
}

/// Coroutine context containing Job and Dispatcher
#[derive(Clone)]
pub struct CoroutineContext {
    pub job: Arc<dyn Job>,
    pub dispatcher: DispatcherEnum,
}

impl CoroutineContext {
    pub fn new(job: Arc<dyn Job>, dispatcher: DispatcherEnum) -> Self {
        Self { job, dispatcher }
    }
    
    /// Cancel all coroutines in this context
    pub async fn cancel(&self) {
        self.job.cancel().await;
    }
    
    /// Check if this context is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.job.is_cancelled()
    }
}

/// Standard implementation of CoroutineScope
pub struct StandardCoroutineScope {
    context: CoroutineContext,
}

impl StandardCoroutineScope {
    pub fn new(context: CoroutineContext) -> Self {
        Self { context }
    }
}

impl CoroutineScope for StandardCoroutineScope {
    fn get_coroutine_context(&self) -> &CoroutineContext {
        &self.context
    }
}

/// Global scope that lives for the entire application lifetime
pub struct GlobalScope;

impl GlobalScope {
    /// Get a reference to the global scope
    pub fn instance() -> &'static dyn CoroutineScope {
        &GLOBAL_SCOPE_INSTANCE
    }
}

static GLOBAL_SCOPE_INSTANCE: GlobalScopeImpl = GlobalScopeImpl;

struct GlobalScopeImpl;

impl CoroutineScope for GlobalScopeImpl {
    fn get_coroutine_context(&self) -> &CoroutineContext {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        static mut CONTEXT: Option<CoroutineContext> = None;
        
        unsafe {
            ONCE.call_once(|| {
                let job = Arc::new(super::job::SupervisorJobImpl::new());
                let dispatcher = super::dispatcher::Dispatchers::default();
                CONTEXT = Some(CoroutineContext::new(job, dispatcher));
            });
            CONTEXT.as_ref().unwrap()
        }
    }
}

/// Creates a new coroutine scope with the given context
pub fn coroutine_scope(context: CoroutineContext) -> StandardCoroutineScope {
    StandardCoroutineScope::new(context)
}

/// Run a block with a new coroutine scope
/// Equivalent to Kotlin's coroutineScope { }
pub async fn with_coroutine_scope<F, T>(f: F) -> Result<T, CancellationException>
where
    F: FnOnce(&dyn CoroutineScope) -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>,
{
    let job = Arc::new(super::job::JobImpl::new());
    let dispatcher = super::dispatcher::Dispatchers::default();
    let context = CoroutineContext::new(job.clone(), dispatcher);
    let scope = StandardCoroutineScope::new(context);
    
    let future = f(&scope);
    
    tokio::select! {
        result = future => Ok(result),
        _ = job.wait_for_cancellation() => {
            Err(CancellationException::new("Coroutine scope was cancelled"))
        }
    }
}