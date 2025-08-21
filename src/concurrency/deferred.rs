use super::{Job, CancellationException};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};

/// A non-blocking cancellable future that represents a deferred result
/// Equivalent to Kotlin's Deferred interface
pub trait Deferred<T>: Job {
    /// Await the result of this deferred
    fn await_result(&self) -> Pin<Box<dyn Future<Output = Result<T, CancellationException>> + Send + '_>>;
    
    /// Get the completed result if available, None if still running
    fn get_completed(&self) -> Pin<Box<dyn Future<Output = Option<Result<T, CancellationException>>> + Send + '_>>;
}

#[derive(Debug)]
enum DeferredState<T> where T: Clone {
    Active,
    Completed(T),
    Failed(CancellationException),
    Cancelled,
}

impl<T: Clone> Clone for DeferredState<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Active => Self::Active,
            Self::Completed(t) => Self::Completed(t.clone()),
            Self::Failed(e) => Self::Failed(e.clone()),
            Self::Cancelled => Self::Cancelled,
        }
    }
}

/// Standard implementation of Deferred
pub struct DeferredImpl<T: Send + Sync + Clone> {
    job: Arc<super::job::JobImpl>,
    state: Arc<RwLock<DeferredState<T>>>,
    completion_notify: Arc<Notify>,
}

impl<T: Send + Sync + Clone + 'static> DeferredImpl<T> {
    pub fn new() -> Self {
        Self {
            job: Arc::new(super::job::JobImpl::new()),
            state: Arc::new(RwLock::new(DeferredState::Active)),
            completion_notify: Arc::new(Notify::new()),
        }
    }
    
    /// Complete this deferred with a result
    pub async fn complete(&self, result: T) {
        {
            let mut state = self.state.write().await;
            if matches!(*state, DeferredState::Active) {
                *state = DeferredState::Completed(result);
                self.completion_notify.notify_waiters();
            }
        }
    }
    
    /// Complete this deferred with an exception
    pub async fn complete_exceptionally(&self, exception: CancellationException) {
        {
            let mut state = self.state.write().await;
            if matches!(*state, DeferredState::Active) {
                *state = DeferredState::Failed(exception);
                self.completion_notify.notify_waiters();
            }
        }
    }
    
    /// Cancel this deferred
    pub async fn cancel_deferred(&self) {
        {
            let mut state = self.state.write().await;
            if matches!(*state, DeferredState::Active) {
                *state = DeferredState::Cancelled;
                self.completion_notify.notify_waiters();
            }
        }
        self.job.cancel().await;
    }
}

impl<T: Send + Sync + Clone + 'static> Job for DeferredImpl<T> {
    fn start(&self) {
        self.job.start();
    }
    
    fn cancel(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.cancel_deferred().await;
        })
    }
    
    fn cancel_with_cause(&self, cause: CancellationException) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            {
                let mut state = self.state.write().await;
                if matches!(*state, DeferredState::Active) {
                    *state = DeferredState::Failed(cause);
                    self.completion_notify.notify_waiters();
                }
            }
            self.job.cancel().await;
        })
    }
    
    fn is_active(&self) -> bool {
        let state = futures::executor::block_on(async {
            let state = self.state.read().await;
            state.clone()
        });
        matches!(state, DeferredState::Active)
    }
    
    fn is_completed(&self) -> bool {
        let state = futures::executor::block_on(async {
            let state = self.state.read().await;
            state.clone()
        });
        !matches!(state, DeferredState::Active)
    }
    
    fn is_cancelled(&self) -> bool {
        let state = futures::executor::block_on(async {
            let state = self.state.read().await;
            state.clone()
        });
        matches!(state, DeferredState::Cancelled) || self.job.is_cancelled()
    }
    
    fn join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            if self.is_completed() {
                return;
            }
            self.completion_notify.notified().await;
        })
    }
    
    fn attach_child(&self, child: Arc<dyn Job>) {
        self.job.attach_child(child);
    }
    
    fn wait_for_cancellation(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.job.wait_for_cancellation()
    }
}

impl<T: Send + Sync + Clone + 'static> Deferred<T> for DeferredImpl<T> {
    fn await_result(&self) -> Pin<Box<dyn Future<Output = Result<T, CancellationException>> + Send + '_>> {
        Box::pin(async move {
            loop {
                {
                    let state = self.state.read().await;
                    match &*state {
                        DeferredState::Completed(result) => return Ok(result.clone()),
                        DeferredState::Failed(exception) => return Err(exception.clone()),
                        DeferredState::Cancelled => {
                            return Err(CancellationException::new("Deferred was cancelled"));
                        }
                        DeferredState::Active => {
                            // Continue waiting
                        }
                    }
                }
                
                self.completion_notify.notified().await;
            }
        })
    }
    
    fn get_completed(&self) -> Pin<Box<dyn Future<Output = Option<Result<T, CancellationException>>> + Send + '_>> {
        Box::pin(async move {
            let state = self.state.read().await;
            match &*state {
                DeferredState::Completed(result) => Some(Ok(result.clone())),
                DeferredState::Failed(exception) => Some(Err(exception.clone())),
                DeferredState::Cancelled => Some(Err(CancellationException::new("Deferred was cancelled"))),
                DeferredState::Active => None,
            }
        })
    }
}

/// Create a completed deferred with the given value
pub fn completed_deferred<T: Send + Sync + Clone + 'static>(value: T) -> impl Deferred<T> {
    let deferred = DeferredImpl::new();
    tokio::spawn({
        let deferred = deferred.clone();
        async move {
            deferred.complete(value).await;
        }
    });
    deferred
}

/// Create a deferred that's completed exceptionally
pub fn failed_deferred<T: Send + Sync + Clone + 'static>(exception: CancellationException) -> impl Deferred<T> {
    let deferred = DeferredImpl::new();
    tokio::spawn({
        let deferred = deferred.clone();
        async move {
            deferred.complete_exceptionally(exception).await;
        }
    });
    deferred
}

impl<T: Send + Sync + Clone + 'static> Clone for DeferredImpl<T> {
    fn clone(&self) -> Self {
        Self {
            job: self.job.clone(),
            state: self.state.clone(),
            completion_notify: self.completion_notify.clone(),
        }
    }
}