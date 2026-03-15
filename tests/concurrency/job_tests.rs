use tokio::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

// Mock Job implementation for testing
struct Job {
    is_cancelled: bool,
    children: Vec<Arc<Job>>,
}

impl Job {
    fn new() -> Self {
        Job {
            is_cancelled: false,
            children: Vec::new(),
        }
    }

    fn cancel(&mut self) {
        self.is_cancelled = true;
        // Cancel all child jobs
        for child in &mut self.children {
            Arc::get_mut(child).unwrap().cancel();
        }
    }

    fn add_child(&mut self, child: Arc<Job>) {
        self.children.push(child);
    }

    fn is_active(&self) -> bool {
        !self.is_cancelled
    }
}

#[tokio::test]
async fn test_job_cancellation() {
    let mut parent_job = Job::new();
    
    // Create child jobs
    let child1 = Arc::new(Job::new());
    let child2 = Arc::new(Job::new());
    
    // Add children to parent
    parent_job.add_child(Arc::clone(&child1));
    parent_job.add_child(Arc::clone(&child2));
    
    // Verify initial state
    assert!(parent_job.is_active());
    assert!(child1.is_active());
    assert!(child2.is_active());
    
    // Cancel parent job
    parent_job.cancel();
    
    // Verify cancellation propagation
    assert!(!parent_job.is_active());
    assert!(!child1.is_active());
    assert!(!child2.is_active());
}

#[tokio::test]
async fn test_supervisor_job_behavior() {
    struct SupervisorJob {
        is_cancelled: bool,
        children: Vec<Arc<Job>>,
    }

    impl SupervisorJob {
        fn new() -> Self {
            SupervisorJob {
                is_cancelled: false,
                children: Vec::new(),
            }
        }

        fn cancel(&mut self) {
            self.is_cancelled = true;
            // Supervisor job does NOT cancel children
        }

        fn add_child(&mut self, child: Arc<Job>) {
            self.children.push(child);
        }

        fn is_active(&self) -> bool {
            !self.is_cancelled
        }
    }

    let mut supervisor_job = SupervisorJob::new();
    
    // Create child jobs
    let child1 = Arc::new(Job::new());
    let child2 = Arc::new(Job::new());
    
    // Add children to supervisor
    supervisor_job.add_child(Arc::clone(&child1));
    supervisor_job.add_child(Arc::clone(&child2));
    
    // Verify initial state
    assert!(supervisor_job.is_active());
    assert!(child1.is_active());
    assert!(child2.is_active());
    
    // Cancel supervisor job
    supervisor_job.cancel();
    
    // Verify children remain active
    assert!(!supervisor_job.is_active());
    assert!(child1.is_active());
    assert!(child2.is_active());
}

#[tokio::test]
async fn test_job_with_async_cancellation() {
    let (tx, mut rx) = mpsc::channel(1);

    let job = Arc::new(tokio::sync::Mutex::new(Job::new()));
    
    // Simulate a long-running async task
    let job_clone = Arc::clone(&job);
    let handle = tokio::spawn(async move {
        // Simulate some work
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        // Check if job is still active
        let mut job_lock = job_clone.lock().await;
        if job_lock.is_active() {
            tx.send(true).await.unwrap();
        } else {
            tx.send(false).await.unwrap();
        }
    });

    // Cancel job before completion
    {
        let mut job_lock = job.lock().await;
        job_lock.cancel();
    }

    // Wait for task to complete
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Check cancellation result
    let result = rx.recv().await.unwrap();
    assert!(!result);

    handle.abort();
}