use crate::concurrency::dispatcher::{dispatch, CoroutineDispatcher};
use std::future::Future;
use std::pin::Pin;
use tokio::task::JoinHandle;
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
    fn dispatch_boxed(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) -> JoinHandle<()> {
        // Mark that the dispatcher was used, then spawn the future.
        self.called.store(true, Ordering::SeqCst);
        tokio::spawn(future)
    }

    fn is_dispatch_needed(&self) -> bool { true }

    fn limit_parallelism(&self, parallelism: usize) -> crate::concurrency::dispatcher::DispatcherEnum {
        crate::concurrency::dispatcher::DispatcherEnum::Default(crate::concurrency::dispatcher::DefaultDispatcher)
    }
}

#[tokio::test]
async fn dispatch_forwards_to_dispatcher() {
    let (spy, called) = SpyDispatcher::new();

    let h = dispatch(&spy, async { 123u32 });

    let v = h.await.unwrap();
    assert_eq!(v, 123u32);
    assert!(called.load(Ordering::SeqCst), "dispatcher was not used by dispatch()");
}
