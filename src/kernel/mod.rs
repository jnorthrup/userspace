//! Kernel emulation modules for userspace implementations of kernel features
//! 
//! This module provides userspace implementations of kernel-level features like
//! io_uring, NIO, and eBPF JIT compilation without requiring kernel dependencies.

#[cfg(all(feature = "kernel", target_os = "linux"))]
pub mod io_uring;

#[cfg(feature = "kernel")]
pub mod nio;

#[cfg(feature = "kernel-ebpf")]
pub mod ebpf;

#[cfg(feature = "kernel-ebpf")]
pub mod ebpf_mmap;

#[cfg(feature = "kernel")]
pub mod densified_ops;

// Export io_uring types when kernel feature is enabled
// Note: The current io_uring module exports KernelUring, not UserIoUring
// We'll need to fix the exports or create aliases

#[cfg(feature = "kernel")]
pub use nio::{NioChannel, SimpleReactor};