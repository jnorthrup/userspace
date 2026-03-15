use crate::concurrency::job::JobImpl;
use crate::concurrency::cancel::CancellationException;
use std::sync::Arc;

#[tokio::test]
async fn parent_cancel_propagates_to_child() {
    let parent = Arc::new(JobImpl::new());
    let child = Arc::new(JobImpl::new());

    // Attach child to parent synchronously
    parent.attach_child(child.clone());

    // cancel parent
    parent.cancel().await;

    // child should be cancelled as well
    assert!(child.is_cancelled(), "child was not cancelled when parent was cancelled");
}
