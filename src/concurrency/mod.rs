//! Structured concurrency patterns mirroring Kotlin coroutines
//!
//! This module provides call-for-call identical abstractions to Kotlin coroutines,
//! implementing structured concurrency patterns with hierarchical cancellation,
//! coroutine scopes, deferred results, and channels.

pub mod scope;
pub mod dispatcher;
pub mod job;
pub mod deferred;
pub mod launch;
pub mod cancel;
pub mod channels;

pub use scope::*;
pub use dispatcher::*;
pub use job::*;
pub use deferred::*;
pub use launch::*;
pub use cancel::*;
pub use channels::*;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Result type for coroutine operations that can be cancelled
pub type CoroutineResult<T> = Result<T, CancellationException>;

/// Exception thrown when a coroutine is cancelled
#[derive(Debug, Clone, PartialEq)]
pub struct CancellationException {
    pub message: String,
}

impl CancellationException {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CancellationException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CancellationException: {}", self.message)
    }
}

impl std::error::Error for CancellationException {}

/// Trait for objects that can be suspended (awaited)
pub trait Suspendable {
    type Output;
    
    fn poll_suspend(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}

impl<F: Future> Suspendable for F {
    type Output = F::Output;
    
    fn poll_suspend(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.poll(cx)
    }
}