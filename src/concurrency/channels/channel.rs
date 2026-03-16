//! Channel primitives for structured concurrency (Kotlin-style)
//!
//! Provides channel types for communicating between coroutines.

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelCapacity {
    Unbounded,
    Buffered(usize),
    Rendezvous,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SendError<T> {
    Closed(T),
    Full(T),
}

impl<T> SendError<T> {
    pub fn into_inner(self) -> T {
        match self {
            Self::Closed(v) | Self::Full(v) => v,
        }
    }
}

impl<T: fmt::Debug> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed(_) => write!(f, "Channel is closed"),
            Self::Full(_) => write!(f, "Channel is full"),
        }
    }
}

impl<T: fmt::Debug> std::error::Error for SendError<T> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvError {
    Empty,
    Closed,
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "Channel is empty"),
            Self::Closed => write!(f, "Channel is closed"),
        }
    }
}

impl std::error::Error for RecvError {}

pub struct SendFuture<T> {
    channel: Arc<dyn Channel<T>>,
    value: Option<T>,
}

impl<T: Send> Future for SendFuture<T> {
    type Output = Result<(), SendError<T>>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let value = unsafe { self.as_mut().get_unchecked_mut() }.value.take().expect("polled after completion");
        match self.channel.try_send(value) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(SendError::Full(v)) => {
                unsafe { self.get_unchecked_mut() }.value = Some(v);
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

pub struct RecvFuture<T> {
    channel: Arc<dyn Channel<T>>,
}

impl<T: Send> Future for RecvFuture<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.channel.try_recv() {
            Ok(v) => Poll::Ready(Ok(v)),
            Err(RecvError::Empty) => Poll::Pending,
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

pub trait Channel<T: Send>: Send + Sync {
    fn poll_send(self: Pin<&mut Self>, cx: &mut Context<'_>, value: T) -> Poll<Result<(), SendError<T>>>;
    fn poll_recv(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>>;
    fn try_send(&self, value: T) -> Result<(), SendError<T>>;
    fn try_recv(&self) -> Result<T, RecvError>;
    fn close(&self);
    fn is_closed(&self) -> bool;
    fn capacity(&self) -> ChannelCapacity;
}

pub struct RendezvousChannel<T: Send> {
    buffer: Arc<Mutex<VecDeque<T>>>,
    closed: Arc<AtomicBool>,
    sender_waker: Arc<Mutex<Option<Waker>>>,
    receiver_waker: Arc<Mutex<Option<Waker>>>,
}

impl<T: Send> RendezvousChannel<T> {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            closed: Arc::new(AtomicBool::new(false)),
            sender_waker: Arc::new(Mutex::new(None)),
            receiver_waker: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T: Send> Default for RendezvousChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + fmt::Debug> fmt::Debug for RendezvousChannel<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RendezvousChannel")
            .field("closed", &self.closed.load(Ordering::SeqCst))
            .finish()
    }
}

impl<T: Send> Clone for RendezvousChannel<T> {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            closed: self.closed.clone(),
            sender_waker: self.sender_waker.clone(),
            receiver_waker: self.receiver_waker.clone(),
        }
    }
}

impl<T: Send> Channel<T> for RendezvousChannel<T> {
    fn poll_send(self: Pin<&mut Self>, cx: &mut Context<'_>, value: T) -> Poll<Result<(), SendError<T>>> {
        let this = self.get_mut();

        if this.closed.load(Ordering::SeqCst) {
            return Poll::Ready(Err(SendError::Closed(value)));
        }

        let mut buffer = this.buffer.lock().unwrap();

        if !buffer.is_empty() {
            buffer.push_back(value);
            drop(buffer);
            if let Ok(mut waker) = this.receiver_waker.lock() {
                if let Some(w) = waker.take() {
                    w.wake();
                }
            }
            return Poll::Ready(Ok(()));
        }

        if let Ok(mut waker) = this.sender_waker.lock() {
            *waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }

    fn poll_recv(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        let this = self.get_mut();
        let mut buffer = this.buffer.lock().unwrap();

        if let Some(value) = buffer.pop_front() {
            drop(buffer);
            if let Ok(mut waker) = this.sender_waker.lock() {
                if let Some(w) = waker.take() {
                    w.wake_by_ref();
                }
            }
            return Poll::Ready(Ok(value));
        }

        if this.closed.load(Ordering::SeqCst) {
            return Poll::Ready(Err(RecvError::Closed));
        }

        if let Ok(mut waker) = this.receiver_waker.lock() {
            *waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(waker) = self.sender_waker.lock() {
            if let Some(w) = waker.as_ref() {
                w.wake_by_ref();
            }
        }
        if let Ok(waker) = self.receiver_waker.lock() {
            if let Some(w) = waker.as_ref() {
                w.wake_by_ref();
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn capacity(&self) -> ChannelCapacity {
        ChannelCapacity::Rendezvous
    }

    fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(SendError::Closed(value));
        }
        let mut buffer = self.buffer.lock().unwrap();
        if !buffer.is_empty() {
            buffer.push_back(value);
            Ok(())
        } else {
            Err(SendError::Full(value))
        }
    }

    fn try_recv(&self) -> Result<T, RecvError> {
        let mut buffer = self.buffer.lock().unwrap();
        if let Some(value) = buffer.pop_front() {
            Ok(value)
        } else if self.closed.load(Ordering::SeqCst) {
            Err(RecvError::Closed)
        } else {
            Err(RecvError::Empty)
        }
    }
}

impl<T: Send> RendezvousChannel<T> {
    pub fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(SendError::Closed(value));
        }
        let mut buffer = self.buffer.lock().unwrap();
        if !buffer.is_empty() {
            buffer.push_back(value);
            Ok(())
        } else {
            Err(SendError::Full(value))
        }
    }

    pub fn try_recv(&self) -> Result<T, RecvError> {
        let mut buffer = self.buffer.lock().unwrap();
        if let Some(value) = buffer.pop_front() {
            Ok(value)
        } else if self.closed.load(Ordering::SeqCst) {
            Err(RecvError::Closed)
        } else {
            Err(RecvError::Empty)
        }
    }
}

pub struct BufferedChannel<T: Send> {
    buffer: Arc<Mutex<VecDeque<T>>>,
    capacity: usize,
    closed: Arc<AtomicBool>,
    sender_waker: Arc<Mutex<Option<Waker>>>,
    receiver_waker: Arc<Mutex<Option<Waker>>>,
}

impl<T: Send> BufferedChannel<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
            closed: Arc::new(AtomicBool::new(false)),
            sender_waker: Arc::new(Mutex::new(None)),
            receiver_waker: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T: Send + fmt::Debug> fmt::Debug for BufferedChannel<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.buffer.lock().map(|b| b.len()).unwrap_or(0);
        f.debug_struct("BufferedChannel")
            .field("capacity", &self.capacity)
            .field("size", &len)
            .finish()
    }
}

impl<T: Send> Clone for BufferedChannel<T> {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            capacity: self.capacity,
            closed: self.closed.clone(),
            sender_waker: self.sender_waker.clone(),
            receiver_waker: self.receiver_waker.clone(),
        }
    }
}

impl<T: Send> Channel<T> for BufferedChannel<T> {
    fn poll_send(self: Pin<&mut Self>, cx: &mut Context<'_>, value: T) -> Poll<Result<(), SendError<T>>> {
        let this = self.get_mut();

        if this.closed.load(Ordering::SeqCst) {
            return Poll::Ready(Err(SendError::Closed(value)));
        }

        let mut buffer = this.buffer.lock().unwrap();

        if buffer.len() < this.capacity {
            buffer.push_back(value);
            drop(buffer);
            if let Ok(mut waker) = this.receiver_waker.lock() {
                if let Some(w) = waker.take() {
                    w.wake();
                }
            }
            return Poll::Ready(Ok(()));
        }

        if let Ok(mut waker) = this.sender_waker.lock() {
            *waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }

    fn poll_recv(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        let this = self.get_mut();
        let mut buffer = this.buffer.lock().unwrap();

        if let Some(value) = buffer.pop_front() {
            drop(buffer);
            if let Ok(mut waker) = this.sender_waker.lock() {
                if let Some(w) = waker.take() {
                    w.wake_by_ref();
                }
            }
            return Poll::Ready(Ok(value));
        }

        if this.closed.load(Ordering::SeqCst) {
            return Poll::Ready(Err(RecvError::Closed));
        }

        if let Ok(mut waker) = this.receiver_waker.lock() {
            *waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(waker) = self.sender_waker.lock() {
            if let Some(w) = waker.as_ref() {
                w.wake_by_ref();
            }
        }
        if let Ok(waker) = self.receiver_waker.lock() {
            if let Some(w) = waker.as_ref() {
                w.wake_by_ref();
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn capacity(&self) -> ChannelCapacity {
        ChannelCapacity::Buffered(self.capacity)
    }

    fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(SendError::Closed(value));
        }
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.len() < self.capacity {
            buffer.push_back(value);
            Ok(())
        } else {
            Err(SendError::Full(value))
        }
    }

    fn try_recv(&self) -> Result<T, RecvError> {
        let mut buffer = self.buffer.lock().unwrap();
        if let Some(value) = buffer.pop_front() {
            Ok(value)
        } else if self.closed.load(Ordering::SeqCst) {
            Err(RecvError::Closed)
        } else {
            Err(RecvError::Empty)
        }
    }
}

impl<T: Send> BufferedChannel<T> {
    pub fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(SendError::Closed(value));
        }
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.len() < self.capacity {
            buffer.push_back(value);
            Ok(())
        } else {
            Err(SendError::Full(value))
        }
    }

    pub fn try_recv(&self) -> Result<T, RecvError> {
        let mut buffer = self.buffer.lock().unwrap();
        if let Some(value) = buffer.pop_front() {
            Ok(value)
        } else if self.closed.load(Ordering::SeqCst) {
            Err(RecvError::Closed)
        } else {
            Err(RecvError::Empty)
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.lock().map(|b| b.len()).unwrap_or(0)
    }
}

pub struct UnboundedChannel<T: Send> {
    buffer: Arc<Mutex<VecDeque<T>>>,
    closed: Arc<AtomicBool>,
    receiver_waker: Arc<Mutex<Option<Waker>>>,
}

impl<T: Send> UnboundedChannel<T> {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            closed: Arc::new(AtomicBool::new(false)),
            receiver_waker: Arc::new(Mutex::new(None)),
        }
    }
}

impl<T: Send> Default for UnboundedChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + fmt::Debug> fmt::Debug for UnboundedChannel<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.buffer.lock().map(|b| b.len()).unwrap_or(0);
        f.debug_struct("UnboundedChannel")
            .field("size", &len)
            .finish()
    }
}

impl<T: Send> Clone for UnboundedChannel<T> {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            closed: self.closed.clone(),
            receiver_waker: self.receiver_waker.clone(),
        }
    }
}

impl<T: Send> Channel<T> for UnboundedChannel<T> {
    fn poll_send(self: Pin<&mut Self>, _cx: &mut Context<'_>, value: T) -> Poll<Result<(), SendError<T>>> {
        let this = self.get_mut();

        if this.closed.load(Ordering::SeqCst) {
            return Poll::Ready(Err(SendError::Closed(value)));
        }

        let mut buffer = this.buffer.lock().unwrap();
        buffer.push_back(value);

        if let Ok(mut waker) = this.receiver_waker.lock() {
            if let Some(w) = waker.take() {
                w.wake();
            }
        }
        Poll::Ready(Ok(()))
    }

    fn poll_recv(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        let this = self.get_mut();
        let mut buffer = this.buffer.lock().unwrap();

        if let Some(value) = buffer.pop_front() {
            return Poll::Ready(Ok(value));
        }

        if this.closed.load(Ordering::SeqCst) {
            return Poll::Ready(Err(RecvError::Closed));
        }

        if let Ok(mut waker) = this.receiver_waker.lock() {
            *waker = Some(cx.waker().clone());
        }
        Poll::Pending
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(waker) = self.receiver_waker.lock() {
            if let Some(w) = waker.as_ref() {
                w.wake_by_ref();
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn capacity(&self) -> ChannelCapacity {
        ChannelCapacity::Unbounded
    }

    fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(SendError::Closed(value));
        }
        let mut buffer = self.buffer.lock().unwrap();
        buffer.push_back(value);
        Ok(())
    }

    fn try_recv(&self) -> Result<T, RecvError> {
        let mut buffer = self.buffer.lock().unwrap();
        if let Some(value) = buffer.pop_front() {
            Ok(value)
        } else if self.closed.load(Ordering::SeqCst) {
            Err(RecvError::Closed)
        } else {
            Err(RecvError::Empty)
        }
    }
}

impl<T: Send> UnboundedChannel<T> {
    pub fn try_send(&self, value: T) -> Result<(), SendError<T>> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(SendError::Closed(value));
        }
        let mut buffer = self.buffer.lock().unwrap();
        buffer.push_back(value);
        Ok(())
    }

    pub fn try_recv(&self) -> Result<T, RecvError> {
        let mut buffer = self.buffer.lock().unwrap();
        if let Some(value) = buffer.pop_front() {
            Ok(value)
        } else if self.closed.load(Ordering::SeqCst) {
            Err(RecvError::Closed)
        } else {
            Err(RecvError::Empty)
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.lock().map(|b| b.len()).unwrap_or(0)
    }
}

pub fn channel<T: Send + 'static>() -> (Sender<T>, Receiver<T>) {
    let ch = Arc::new(RendezvousChannel::new());
    (Sender(ch.clone()), Receiver(ch))
}

pub fn buffered_channel<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let ch = Arc::new(BufferedChannel::new(capacity));
    (Sender(ch.clone()), Receiver(ch))
}

pub fn unbounded_channel<T: Send + 'static>() -> (Sender<T>, Receiver<T>) {
    let ch = Arc::new(UnboundedChannel::new());
    (Sender(ch.clone()), Receiver(ch))
}

#[derive(Clone)]
pub struct Sender<T: Send>(Arc<dyn Channel<T>>);

impl<T: Send> Sender<T> {
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        SendFuture {
            channel: self.0.clone(),
            value: Some(value),
        }.await
    }

    pub fn try_send(&self, value: T) -> Result<(), SendError<T>>
    where
        T: 'static,
    {
        self.0.try_send(value)
    }

    pub fn close(&self) {
        self.0.close()
    }

    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    pub fn capacity(&self) -> ChannelCapacity {
        self.0.capacity()
    }
}

pub struct Receiver<T: Send>(Arc<dyn Channel<T>>);

impl<T: Send> Receiver<T> {
    pub async fn recv(&self) -> Result<T, RecvError> {
        RecvFuture {
            channel: self.0.clone(),
        }.await
    }

    pub fn capacity(&self) -> ChannelCapacity {
        self.0.capacity()
    }

    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    pub fn close(&self) {
        self.0.close()
    }
}

#[allow(dead_code)]
trait AnyChannel: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: Send + 'static> AnyChannel for RendezvousChannel<T> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl<T: Send + 'static> AnyChannel for BufferedChannel<T> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl<T: Send + 'static> AnyChannel for UnboundedChannel<T> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rendezvous_channel() {
        let (tx, rx) = channel::<i32>();
        
        let handle = tokio::spawn(async move {
            tx.send(42).await.unwrap();
        });
        
        let value = rx.recv().await.unwrap();
        assert_eq!(value, 42);
        
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_buffered_channel() {
        let (tx, rx) = buffered_channel(2);
        
        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();
        
        assert!(tx.try_send(3).is_err());
        
        assert_eq!(rx.recv().await.unwrap(), 1);
        assert_eq!(rx.recv().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_unbounded_channel() {
        let (tx, rx) = unbounded_channel::<i32>();
        
        for i in 0..100 {
            tx.send(i).await.unwrap();
        }
        
        for i in 0..100 {
            assert_eq!(rx.recv().await.unwrap(), i);
        }
    }

    #[tokio::test]
    async fn test_channel_close() {
        let (tx, rx) = channel::<i32>();
        
        tx.close();
        
        assert!(tx.send(1).await.is_err());
        assert!(rx.recv().await.is_err());
    }

    #[test]
    fn test_channel_capacity() {
        let (tx1, _) = channel::<i32>();
        let (tx2, _) = buffered_channel::<i32>(5);
        let (tx3, _) = unbounded_channel::<i32>();

        assert_eq!(tx1.capacity(), ChannelCapacity::Rendezvous);
        assert_eq!(tx2.capacity(), ChannelCapacity::Buffered(5));
        assert_eq!(tx3.capacity(), ChannelCapacity::Unbounded);
    }
}
