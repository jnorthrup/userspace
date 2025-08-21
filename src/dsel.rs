//! DSEL helpers: taxonomical import aliases, newtypes, and zero-cost scaffolding for
//! conditional composition, introspection and debug-only utilities.
//!
//! This module is feature-gated behind `dsel` to ensure zero-cost by default.

#[cfg(feature = "dsel")]
// Real implementations when feature enabled
pub mod dsel_impl {
    use std::sync::Arc;

    // --- Taxonomical import aliases ---
    // These are convenience re-exports so users can pick a domain-specific path.
    pub mod lifecycle {
        pub use crate::concurrency::job::{Job, JobImpl, SupervisorJobImpl};
        pub use crate::concurrency::cancel::{CancellationToken, ensure_active, is_active};
    }

    pub mod concurrency {
        pub use crate::concurrency::dispatcher::{CoroutineDispatcher, Dispatchers, DispatcherEnum};
        pub use crate::concurrency::launch::{launch, async_coroutine, run_blocking};
    }

    pub mod completion {
        pub use crate::concurrency::deferred::{Deferred, DeferredImpl, completed_deferred, failed_deferred};
    }

    // --- Newtypes for zero-cost tagging and conditional behavior ---
    // These wrappers are repr(transparent) and should compile away in optimized builds.
    #[repr(transparent)]
    #[derive(Clone)]
    pub struct Scoped<'a, S: crate::concurrency::scope::CoroutineScope + ?Sized>(pub &'a S);

    impl<'a, S: crate::concurrency::scope::CoroutineScope + ?Sized> Scoped<'a, S> {
        pub fn context(&self) -> &'a crate::concurrency::scope::CoroutineContext {
            self.0.get_coroutine_context()
        }
    }

    // Newtype for a debugging handle around a Job
    #[repr(transparent)]
    #[derive(Clone)]
    pub struct JobHandle(pub Arc<dyn crate::concurrency::job::Job>);

    impl JobHandle {
        pub fn is_cancelled(&self) -> bool {
            self.0.is_cancelled()
        }

        pub fn is_completed(&self) -> bool {
            self.0.is_completed()
        }
    }

    // Zero-cost conversion helpers
    pub fn scoped<'a, S: crate::concurrency::scope::CoroutineScope + ?Sized>(s: &'a S) -> Scoped<'a, S> {
        Scoped(s)
    }

    pub fn handle_from_job(j: Arc<dyn crate::concurrency::job::Job>) -> JobHandle {
        JobHandle(j)
    }

    // --- Conditional debug/introspection helpers ---
    #[cfg(feature = "dsel-debug")]
    pub mod debug {
        use super::*;
        use std::sync::Arc;

        /// Returns a human-friendly snapshot of a coroutine context and job
        pub fn snapshot_context(ctx: &crate::concurrency::scope::CoroutineContext) -> String {
            format!(
                "CoroutineContext {{ cancelled: {}, is_completed: {} }}",
                ctx.is_cancelled(),
                ctx.job.is_completed()
            )
        }

        /// Pretty print a deferred state (best-effort)
        pub async fn inspect_deferred<T: Clone + std::fmt::Debug>(d: &Arc<dyn crate::concurrency::deferred::Deferred<T>>) -> String {
            match d.get_completed().await {
                None => "Deferred: Active".to_string(),
                Some(Ok(v)) => format!("Deferred: Completed({:?})", v),
                Some(Err(e)) => format!("Deferred: Failed({})", e),
            }
        }

        /// Attach a tracing span around a future for temporary debugging
        pub fn trace_future<F, T>(name: &str, fut: F) -> impl std::future::Future<Output = T>
        where
            F: std::future::Future<Output = T>,
        {
            let span = tracing::span!(tracing::Level::DEBUG, "dsel::future", name = name);
            async move {
                let _enter = span.enter();
                fut.await
            }
        }
    }
}

#[cfg(not(feature = "dsel"))]
// Provide zero-size stubs so code that conditionally imports types still compiles
pub mod dsel_impl {
    // Empty stubs; users must enable `dsel` feature to get real helpers.
}

// Tests to ensure the DSEL helpers provide zero-cost newtypes and
// the taxonomical aliases compile both when the feature is enabled
// and when it is not. The feature-enabled tests assert size equality
// (repr(transparent) should compile away) and simple method forwarding.
#[cfg(test)]
mod tests {
    use super::dsel_impl;

    #[cfg(not(feature = "dsel"))]
    #[test]
    fn dsel_stub_compiles() {
        // When the feature is disabled the module should still be present
        // as a stub so code that conditionally imports it can compile.
        // Simply verify the module exists by using it in a type context
        fn _test_module_exists() {
            // Module exists if this compiles
        }
    }

    #[cfg(feature = "dsel")]
    mod enabled {
        use super::dsel_impl;
        use std::mem::size_of;
        use std::sync::Arc;

        #[test]
        fn newtypes_are_zero_cost() {
            // Scoped<'_, S> should be transparent over &S
            let scope = crate::concurrency::scope::StandardCoroutineScope::new(
                crate::concurrency::scope::CoroutineContext::new(
                    Arc::new(crate::concurrency::job::JobImpl::new()),
                    crate::concurrency::dispatcher::Dispatchers::default(),
                ),
            );
            let scoped = dsel_impl::Scoped(&scope);
            // sizes should match pointer/reference size
            assert_eq!(size_of::<dsel_impl::Scoped<'_, crate::concurrency::scope::StandardCoroutineScope>>(), size_of::<&crate::concurrency::scope::StandardCoroutineScope>());

            // JobHandle should be same size as Arc<dyn Job>
            let job = Arc::new(crate::concurrency::job::JobImpl::new()) as Arc<dyn crate::concurrency::job::Job>;
            let handle = dsel_impl::JobHandle(job.clone());
            assert_eq!(size_of::<dsel_impl::JobHandle>(), size_of::<Arc<dyn crate::concurrency::job::Job>>());
            // method forwarding
            assert_eq!(handle.is_cancelled(), job.is_cancelled());
            assert_eq!(handle.is_completed(), job.is_completed());
        }
    }
}
