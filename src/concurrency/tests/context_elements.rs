use crate::concurrency::*;
use std::sync::Arc;

#[tokio::test]
async fn context_element_set_get() {
    let job = Arc::new(super::job::JobImpl::new());
    let dispatcher = super::dispatcher::Dispatchers::default();
    let ctx = super::scope::CoroutineContext::new(job, dispatcher);

    // store a simple value by type
    ctx.set_typed::<u32>(123u32);

    let v = ctx.get_typed::<u32>().expect("element present");
    let val = (&*v).downcast_ref::<u32>().expect("downcast to u32");
    assert_eq!(*val, 123u32);
}
