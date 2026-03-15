use super::{DispatcherEnum, Job, CancellationException};
use std::sync::Arc;
use std::any::Any;
use std::collections::HashMap;
use std::sync::RwLock as StdRwLock;

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
    // Optional extensible key/value elements attached to the context.
    // Keys are &'static str for simplicity; values are stored as Arc<dyn Any + Send + Sync>.
    pub elements: Arc<StdRwLock<HashMap<std::any::TypeId, Arc<dyn Any + Send + Sync>>>>,
}

impl CoroutineContext {
    pub fn new(job: Arc<dyn Job>, dispatcher: DispatcherEnum) -> Self {
        Self {
            job,
            dispatcher,
            elements: Arc::new(StdRwLock::new(HashMap::new())),
        }
    }
    
    /// Cancel all coroutines in this context
    pub async fn cancel(&self) {
        self.job.cancel().await;
    }
    
    /// Check if this context is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.job.is_cancelled()
    }

    /// Insert or replace a context element keyed by its concrete type.
    pub fn set_typed<T: Any + Send + Sync + 'static>(&self, value: T) {
        let mut guard = self.elements.write().unwrap();
        guard.insert(std::any::TypeId::of::<T>(), Arc::new(value));
    }

    /// Retrieve an element by concrete type. Returns an Arc<dyn Any> which the caller
    /// can downcast (e.g. `(&*arc).downcast_ref::<T>()`).
    pub fn get_typed<T: Any + Send + Sync + 'static>(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        let guard = self.elements.read().unwrap();
        guard.get(&std::any::TypeId::of::<T>()).cloned()
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
        use std::sync::OnceLock;

        static CONTEXT: OnceLock<CoroutineContext> = OnceLock::new();

        CONTEXT.get_or_init(|| {
            let job = Arc::new(super::job::SupervisorJobImpl::new());
            let dispatcher = super::dispatcher::Dispatchers::default();
            CoroutineContext::new(job, dispatcher)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_scope_context_is_static() {
        // Two calls should return the same reference (same address)
        let a = GlobalScope::instance().get_coroutine_context() as *const _;
        let b = GlobalScope::instance().get_coroutine_context() as *const _;
        assert_eq!(a, b);

        // The supervisor job used by the global context should not be cancelled by default
        assert!(!GlobalScope::instance().get_coroutine_context().is_cancelled());
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