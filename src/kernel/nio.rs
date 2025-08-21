//! Userspace non-blocking I/O abstractions
//!
//! This module provides cross-platform facades over common userspace
//! non-blocking I/O primitives: reactors, poll/select fallbacks, and
//! lightweight buffer management.

use std::io;
use std::time::Duration;
use std::task::{Context, Poll};
use std::pin::Pin;
use std::future::Future;

/// Trait representing a non-blocking channel capable of polling for readiness
pub trait NioChannel: Send + Sync {
    /// Poll for readability with an optional timeout
    fn poll_readable(&self, timeout: Option<Duration>) -> io::Result<bool>;
    
    /// Poll for writability with an optional timeout
    fn poll_writable(&self, timeout: Option<Duration>) -> io::Result<bool>;
    
    /// Try to read data without blocking
    fn try_read(&self, buf: &mut [u8]) -> io::Result<usize>;
    
    /// Try to write data without blocking  
    fn try_write(&self, buf: &[u8]) -> io::Result<usize>;
}

/// A reactor for managing non-blocking I/O operations
pub trait Reactor: Send + Sync {
    /// Register a channel for monitoring
    fn register<C: NioChannel + 'static>(&self, channel: C) -> io::Result<usize>;
    
    /// Unregister a channel
    fn unregister(&self, id: usize) -> io::Result<()>;
    
    /// Run the reactor for a single tick
    fn tick(&self, max_wait: Option<Duration>) -> io::Result<usize>;
    
    /// Get number of registered channels
    fn channel_count(&self) -> usize;
}

/// Simple single-threaded reactor implementation
pub struct SimpleReactor {
    channels: parking_lot::RwLock<Vec<Box<dyn NioChannel>>>,
}

impl SimpleReactor {
    pub fn new() -> Self {
        Self {
            channels: parking_lot::RwLock::new(Vec::new()),
        }
    }
    
    /// Process all ready channels
    pub fn process_ready(&self) -> io::Result<usize> {
        let channels = self.channels.read();
        let mut ready_count = 0;
        
        for channel in channels.iter() {
            if channel.poll_readable(Some(Duration::from_millis(0)))? {
                ready_count += 1;
            }
            if channel.poll_writable(Some(Duration::from_millis(0)))? {
                ready_count += 1;
            }
        }
        
        Ok(ready_count)
    }
}

impl Default for SimpleReactor {
    fn default() -> Self {
        Self::new()
    }
}

impl Reactor for SimpleReactor {
    fn register<C: NioChannel + 'static>(&self, channel: C) -> io::Result<usize> {
        let mut channels = self.channels.write();
        let id = channels.len();
        channels.push(Box::new(channel));
        Ok(id)
    }
    
    fn unregister(&self, id: usize) -> io::Result<()> {
        let mut channels = self.channels.write();
        if id < channels.len() {
            channels.remove(id);
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid channel ID"))
        }
    }
    
    fn tick(&self, max_wait: Option<Duration>) -> io::Result<usize> {
        // Simple implementation: check all channels for readiness
        if let Some(duration) = max_wait {
            std::thread::sleep(duration.min(Duration::from_millis(1)));
        }
        self.process_ready()
    }
    
    fn channel_count(&self) -> usize {
        self.channels.read().len()
    }
}

/// Future that waits for a channel to become readable
pub struct ReadableFuture<C: NioChannel> {
    channel: C,
}

impl<C: NioChannel> ReadableFuture<C> {
    pub fn new(channel: C) -> Self {
        Self { channel }
    }
}

impl<C: NioChannel> Future for ReadableFuture<C> {
    type Output = io::Result<()>;
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.channel.poll_readable(Some(Duration::from_millis(0))) {
            Ok(true) => Poll::Ready(Ok(())),
            Ok(false) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Future that waits for a channel to become writable
pub struct WritableFuture<C: NioChannel> {
    channel: C,
}

impl<C: NioChannel> WritableFuture<C> {
    pub fn new(channel: C) -> Self {
        Self { channel }
    }
}

impl<C: NioChannel> Future for WritableFuture<C> {
    type Output = io::Result<()>;
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.channel.poll_writable(Some(Duration::from_millis(0))) {
            Ok(true) => Poll::Ready(Ok(())),
            Ok(false) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

/// Mock channel for testing
#[cfg(test)]
struct MockChannel {
    readable: std::sync::atomic::AtomicBool,
    writable: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl MockChannel {
    fn new(readable: bool, writable: bool) -> Self {
        use std::sync::atomic::AtomicBool;
        Self {
            readable: AtomicBool::new(readable),
            writable: AtomicBool::new(writable),
        }
    }
}

#[cfg(test)]
impl NioChannel for MockChannel {
    fn poll_readable(&self, _timeout: Option<Duration>) -> io::Result<bool> {
        Ok(self.readable.load(std::sync::atomic::Ordering::Acquire))
    }
    
    fn poll_writable(&self, _timeout: Option<Duration>) -> io::Result<bool> {
        Ok(self.writable.load(std::sync::atomic::Ordering::Acquire))
    }
    
    fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if self.readable.load(std::sync::atomic::Ordering::Acquire) {
            buf[0] = 42;
            Ok(1)
        } else {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "Not readable"))
        }
    }
    
    fn try_write(&self, _buf: &[u8]) -> io::Result<usize> {
        if self.writable.load(std::sync::atomic::Ordering::Acquire) {
            Ok(1)
        } else {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "Not writable"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_reactor() {
        let reactor = SimpleReactor::new();
        assert_eq!(reactor.channel_count(), 0);
        
        let channel = MockChannel::new(true, false);
        let id = reactor.register(channel).unwrap();
        assert_eq!(reactor.channel_count(), 1);
        
        let ready = reactor.tick(Some(Duration::from_millis(1))).unwrap();
        assert_eq!(ready, 1); // One channel is readable
        
        reactor.unregister(id).unwrap();
        assert_eq!(reactor.channel_count(), 0);
    }
    
    #[tokio::test]
    async fn test_readable_future() {
        let channel = MockChannel::new(true, false);
        let fut = ReadableFuture::new(channel);
        fut.await.unwrap();
    }
    
    #[tokio::test]
    async fn test_writable_future() {
        let channel = MockChannel::new(false, true);
        let fut = WritableFuture::new(channel);
        fut.await.unwrap();
    }
}