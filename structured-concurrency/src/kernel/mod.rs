//! Kernel emulation modules for userspace implementations of kernel features
//! 
//! This module provides userspace implementations of kernel-level features like
//! io_uring, NIO, and eBPF JIT compilation without requiring kernel dependencies.

#[cfg(feature = "kernel")]
pub mod io_uring;

#[cfg(feature = "kernel")]
pub mod nio;

#[cfg(feature = "kernel-ebpf")]
pub mod ebpf;

#[cfg(feature = "kernel-ebpf")]
pub mod ebpf_mmap;

#[cfg(feature = "kernel")]
pub use io_uring::{UserIoUring, IoOp, OpCode, CompletionEntry};

#[cfg(feature = "kernel")]
pub use nio::{NioChannel, SimpleReactor};