//! Channel primitives for structured concurrency (Kotlin-style)
//!
//! Provides channel types for communicating between coroutines:
//! - `Channel` - unbuffered/bounded channel for send/receive
//! - `RendezvousChannel` - unbuffered channel (default)
//! - `BufferedChannel` - channel with bounded buffer
//! - `BroadcastChannel` - one-to-many channel
//! - `Producer` - coroutine builder for producing values

use std::collections::VecDeque;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use crate::concurrency::CancellationException;
use crate::concurrency::job::Job;

pub mod channel;
pub mod producer;
pub mod broadcast;

pub use channel::*;
pub use producer::*;
pub use broadcast::*;

pub trait Channel<T>: Send + Sync {
    fn send(&self, value: T) -> impl Future<Output = Result<(), SendError<T>>>;
    fn recv(&self) -> impl Future<Output = Result<T, RecvError>>;
    fn close(&self);
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

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed(_) => write!(f, "Channel is closed"),
            Self::Full(_) => write!(f, "Channel is full"),
        }
    }
}

impl<T: fmt::Debug> std::error::Error for SendError<T> {}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelCapacity {
    Unbounded,
    Buffered(usize),
    Rendezvous,
}

impl ChannelCapacity {
    pub fn new(capacity: isize) -> Self {
        match capacity {
            0 => Self::Rendezvous,
            x if x > 0 => Self::Buffered(x as usize),
            _ => Self::Unbounded,
        }
    }
}
