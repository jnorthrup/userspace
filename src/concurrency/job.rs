use super::CancellationException;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};

/// A cancellable job with a lifecycle
/// Equivalent to Kotlin's Job interface
pub trait Job: Send + Sync {
    /// Start the job
    fn start(&self);
    
    /// Cancel the job
    fn cancel(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    
    /// Cancel the job with a specific cause
    fn cancel_with_cause(&self, cause: CancellationException) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    
    /// Check if this job is active
    fn is_active(&self) -> bool;
    
    /// Check if this job is completed
    fn is_completed(&self) -> bool;
    
    /// Check if this job is cancelled
    fn is_cancelled(&self) -> bool;
    
    /// Wait for the job to complete
    fn join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    
    /// Attach a child job
    fn attach_child(&self, child: Arc<dyn Job>);
    
    /// Wait for cancellation
    fn wait_for_cancellation(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum JobState {
    New,
    Active,
    Completing,
    Completed,
    Cancelling,
    Cancelled,
}

/// Standard implementation of Job
pub struct JobImpl {
    state: Arc<RwLock<JobState>>,
    cancelled: Arc<AtomicBool>,
    completed_notify: Arc<Notify>,
    cancel_notify: Arc<Notify>,
    children: Arc<RwLock<Vec<Arc<dyn Job>>>>,
    cause: Arc<RwLock<Option<CancellationException>>>,
}

impl JobImpl {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(JobState::New)),
            cancelled: Arc::new(AtomicBool::new(false)),
            completed_notify: Arc::new(Notify::new()),
            cancel_notify: Arc::new(Notify::new()),
            children: Arc::new(RwLock::new(Vec::new())),
            cause: Arc::new(RwLock::new(None)),
        }
    }
    
    async fn set_state(&self, new_state: JobState) {
        let mut state = self.state.write().await;
        *state = new_state;
        
        match new_state {
            JobState::Completed => {
                self.completed_notify.notify_waiters();
            }
            JobState::Cancelled => {
                self.completed_notify.notify_waiters();
                self.cancelled.store(true, Ordering::SeqCst);
                self.cancel_notify.notify_waiters();
            }
            JobState::Cancelling => {
                self.cancelled.store(true, Ordering::SeqCst);
                self.cancel_notify.notify_waiters();
            }
            _ => {}
        }
    }
}

impl Job for JobImpl {
    fn start(&self) {
        let state = self.state.clone();
        let completed_notify = self.completed_notify.clone();
        
        tokio::spawn(async move {
            let mut state_guard = state.write().await;
            if *state_guard == JobState::New {
                *state_guard = JobState::Active;
            }
        });
    }
    
    fn cancel(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let cause = CancellationException::new("Job was cancelled");
        self.cancel_with_cause(cause)
    }
    
    fn cancel_with_cause(&self, cause: CancellationException) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let current_state = {
                let state = self.state.read().await;
                *state
            };
            
            if current_state == JobState::Completed || current_state == JobState::Cancelled {
                return;
            }
            
            {
                let mut cause_guard = self.cause.write().await;
                *cause_guard = Some(cause);
            }
            
            self.set_state(JobState::Cancelling).await;
            
            // Cancel all children
            let children = {
                let children_guard = self.children.read().await;
                children_guard.clone()
            };
            
            for child in children {
                child.cancel().await;
            }
            
            self.set_state(JobState::Cancelled).await;
        })
    }
    
    fn is_active(&self) -> bool {
        let state = futures::executor::block_on(async {
            let state = self.state.read().await;
            *state
        });
        matches!(state, JobState::Active)
    }
    
    fn is_completed(&self) -> bool {
        let state = futures::executor::block_on(async {
            let state = self.state.read().await;
            *state
        });
        matches!(state, JobState::Completed | JobState::Cancelled)
    }
    
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    
    fn join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            if self.is_completed() {
                return;
            }
            self.completed_notify.notified().await;
        })
    }
    
    fn attach_child(&self, child: Arc<dyn Job>) {
        let children = self.children.clone();
        tokio::spawn(async move {
            let mut children_guard = children.write().await;
            children_guard.push(child);
        });
    }
    
    fn wait_for_cancellation(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            if self.is_cancelled() {
                return;
            }
            self.cancel_notify.notified().await;
        })
    }
}

/// A job that doesn't cancel its children when it's cancelled
pub struct SupervisorJobImpl {
    inner: JobImpl,
}

impl SupervisorJobImpl {
    pub fn new() -> Self {
        Self {
            inner: JobImpl::new(),
        }
    }
}

impl Job for SupervisorJobImpl {
    fn start(&self) {
        self.inner.start();
    }
    
    fn cancel(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            // SupervisorJob only cancels itself, not its children
            let current_state = {
                let state = self.inner.state.read().await;
                *state
            };
            
            if current_state == JobState::Completed || current_state == JobState::Cancelled {
                return;
            }
            
            self.inner.set_state(JobState::Cancelled).await;
        })
    }
    
    fn cancel_with_cause(&self, cause: CancellationException) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            {
                let mut cause_guard = self.inner.cause.write().await;
                *cause_guard = Some(cause);
            }
            self.cancel().await;
        })
    }
    
    fn is_active(&self) -> bool {
        self.inner.is_active()
    }
    
    fn is_completed(&self) -> bool {
        self.inner.is_completed()
    }
    
    fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
    
    fn join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.inner.join()
    }
    
    fn attach_child(&self, child: Arc<dyn Job>) {
        self.inner.attach_child(child);
    }
    
    fn wait_for_cancellation(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.inner.wait_for_cancellation()
    }
}