use super::{CoroutineScope, CancellationException};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::oneshot;

/// Cancellation token that can be used to signal cancellation
pub struct CancellationToken {
    receiver: tokio::sync::broadcast::Receiver<CancellationException>,
    sender: tokio::sync::broadcast::Sender<CancellationException>,
}

impl Clone for CancellationToken {
    fn clone(&self) -> Self {
        Self {
            receiver: self.sender.subscribe(),
            sender: self.sender.clone(),
        }
    }
}

impl CancellationToken {
    /// Create a new cancellation token
    pub fn new() -> Self {
        let (sender, receiver) = tokio::sync::broadcast::channel(1);
        Self { receiver, sender }
    }
    
    /// Cancel this token with a specific cause
    pub fn cancel(&self, cause: CancellationException) {
        let _ = self.sender.send(cause);
    }
    
    /// Check if this token is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.receiver.is_empty() == false
    }
    
    /// Wait for cancellation
    pub async fn cancelled(&mut self) -> CancellationException {
        self.receiver.recv().await
            .unwrap_or_else(|_| CancellationException::new("Token was cancelled"))
    }
    
    /// Create a child token that's cancelled when this token is cancelled
    pub fn child_token(&self) -> CancellationToken {
        let child = CancellationToken::new();
        let mut receiver = self.receiver.resubscribe();
        let child_sender = child.sender.clone();
        
        tokio::spawn(async move {
            if let Ok(cause) = receiver.recv().await {
                let _ = child_sender.send(cause);
            }
        });
        
        child
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Throw a CancellationException if the current coroutine is cancelled
/// Equivalent to Kotlin's ensureActive()
pub async fn ensure_active(scope: &dyn CoroutineScope) -> Result<(), CancellationException> {
    let context = scope.get_coroutine_context();
    if context.is_cancelled() {
        Err(CancellationException::new("Coroutine scope is cancelled"))
    } else {
        Ok(())
    }
}

/// Check if the current coroutine is active (not cancelled)
/// Equivalent to Kotlin's isActive
pub fn is_active(scope: &dyn CoroutineScope) -> bool {
    let context = scope.get_coroutine_context();
    !context.is_cancelled()
}

/// Yield control to other coroutines
/// Equivalent to Kotlin's yield()
pub async fn yield_now() {
    tokio::task::yield_now().await;
}

/// A future that completes when cancelled
pub struct CancellableFuture<F> {
    future: Pin<Box<F>>,
    token: CancellationToken,
}

impl<F> CancellableFuture<F>
where
    F: Future,
{
    pub fn new(future: F, token: CancellationToken) -> Self {
        Self {
            future: Box::pin(future),
            token,
        }
    }
}

impl<F> Future for CancellableFuture<F>
where
    F: Future,
{
    type Output = Result<F::Output, CancellationException>;
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Check for cancellation first
        if self.token.is_cancelled() {
            return Poll::Ready(Err(CancellationException::new("Future was cancelled")));
        }
        
        // Poll the underlying future
        match self.future.as_mut().poll(cx) {
            Poll::Ready(result) => Poll::Ready(Ok(result)),
            Poll::Pending => {
                // Check cancellation again
                if self.token.is_cancelled() {
                    Poll::Ready(Err(CancellationException::new("Future was cancelled")))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

/// Wrap a future to make it cancellable
pub fn cancellable<F>(future: F, token: CancellationToken) -> CancellableFuture<F>
where
    F: Future,
{
    CancellableFuture::new(future, token)
}

/// Suspending function that throws CancellationException if cancelled
/// Equivalent to Kotlin's suspendCancellableCoroutine
pub async fn suspendable_cancellable<T, F>(
    scope: &dyn CoroutineScope,
    block: F,
) -> Result<T, CancellationException>
where
    F: FnOnce(oneshot::Sender<Result<T, CancellationException>>) -> (),
    T: Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    
    // Start the block
    block(sender);
    
    // Race between completion and cancellation
    tokio::select! {
        result = receiver => {
            match result {
                Ok(value) => value,
                Err(_) => Err(CancellationException::new("Suspendable operation was abandoned")),
            }
        }
        _ = scope.get_coroutine_context().job.wait_for_cancellation() => {
            Err(CancellationException::new("Coroutine scope was cancelled"))
        }
    }
}

/// Timeout a coroutine operation
pub async fn with_timeout<F, T>(
    duration: std::time::Duration,
    future: F,
) -> Result<T, CancellationException>
where
    F: Future<Output = T>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(result) => Ok(result),
        Err(_) => Err(CancellationException::new("Operation timed out")),
    }
}

/// Timeout a coroutine operation, returning None on timeout instead of error
pub async fn with_timeout_or_null<F, T>(
    duration: std::time::Duration,
    future: F,
) -> Option<T>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future).await.ok()
}

/// NonCancellable context that ignores cancellation
pub struct NonCancellable;

/// Run a block in a non-cancellable context
/// Equivalent to Kotlin's NonCancellable
pub async fn with_non_cancellable<F, T>(block: F) -> T
where
    F: Future<Output = T>,
{
    // In a real implementation, this would create a special context
    // that ignores cancellation signals
    block.await
}

/// Exception handler for coroutine exceptions
pub trait CoroutineExceptionHandler: Send + Sync {
    fn handle_exception(&self, exception: CancellationException);
}

/// Default exception handler that logs exceptions
pub struct DefaultExceptionHandler;

impl CoroutineExceptionHandler for DefaultExceptionHandler {
    fn handle_exception(&self, exception: CancellationException) {
        tracing::error!("Unhandled coroutine exception: {}", exception);
    }
}