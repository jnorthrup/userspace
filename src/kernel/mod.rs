//! Unified Kernel Abstractions
//!
//! Provides high-performance, zero-overhead interfaces to:
//! - io_uring for kernel I/O
//! - eBPF JIT compilation
//! - Memory-mapped I/O
//! - Kernel bypass techniques
//!
//! Features are conditionally compiled and require explicit opt-in.

// Kernel feature detection and capabilities
pub mod kernel_capabilities;

// Core kernel interface modules
#[cfg(all(feature = "kernel", target_os = "linux"))]
pub mod io_uring;

#[cfg(feature = "kernel")]
pub mod nio;

#[cfg(feature = "syscall-net")]
pub mod syscall_net;

#[cfg(feature = "syscall-net")]
pub mod posix_sockets;

// Performance and optimization modules
#[cfg(feature = "kernel-ebpf")]
pub mod ebpf;

#[cfg(feature = "kernel-ebpf")]
pub mod ebpf_mmap;

#[cfg(feature = "kernel")]
pub mod densified_ops;

#[cfg(feature = "kernel")]
pub mod endgame_bypass;

#[cfg(feature = "kernel")]
pub mod knox_proxy;

#[cfg(feature = "kernel")]
pub mod tethering_bypass;

#[cfg(feature = "kernel")]
pub mod syscall;

#[cfg(feature = "simd")]
pub mod simd_ops;

// Unified type exports
#[cfg(feature = "kernel")]
pub use {
    kernel_capabilities::SystemCapabilities,
    nio::{NioChannel, SimpleReactor},
};

#[cfg(feature = "syscall-net")]
pub use syscall_net::{SocketOps, NetworkInterface};

#[cfg(feature = "syscall-net")]
pub use posix_sockets::{PosixSocket, SocketPair};

#[cfg(feature = "kernel")]
pub use endgame_bypass::{DensifiedKernel, IoUringParams};