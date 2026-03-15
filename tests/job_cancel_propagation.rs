
use userspace::concurrency::job::JobImpl;
use userspace::concurrency::Job;
use std::sync::Arc;

#[tokio::test]
async fn parent_cancel_propagates_to_child() {
    let parent: Arc<dyn Job> = Arc::new(JobImpl::new());
    let child: Arc<dyn Job> = Arc::new(JobImpl::new());

    parent.attach_child(child.clone());

    parent.cancel().await;

    assert!(child.is_cancelled(), "child was not cancelled when parent was cancelled");
}
