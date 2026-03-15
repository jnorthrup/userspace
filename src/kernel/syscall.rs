//! Unified Kernel Syscall Interface
//!
//! Provides a single API surface for all syscall adapters in the kernel module.
//! This module re-exports types from other kernel modules for convenience.

#[cfg(feature = "kernel")]
pub use crate::kernel::endgame_bypass::DensifiedKernel;

#[cfg(feature = "kernel")]
pub use crate::kernel::knox_proxy::{KnoxProxy, TunnelSession};

#[cfg(feature = "kernel")]
pub use crate::kernel::tethering_bypass::{TetheringConfig, TetheringSession};

#[cfg(feature = "syscall-net")]
pub use crate::kernel::syscall_net::{NetworkInterface, SocketOps};

pub trait SyscallAdapter: Send + Sync {
    fn read(&self, fd: std::os::unix::io::RawFd, buf: &mut [u8]) -> std::io::Result<usize>;
    fn write(&self, fd: std::os::unix::io::RawFd, buf: &[u8]) -> std::io::Result<usize>;
    fn close(&self, fd: std::os::unix::io::RawFd) -> std::io::Result<()>;
}

pub trait NetworkAdapter: Send + Sync {
    fn connect(&self, addr: std::net::SocketAddr) -> std::io::Result<std::os::unix::io::RawFd>;
    fn bind(&self, addr: std::net::SocketAddr) -> std::io::Result<std::os::unix::io::RawFd>;
    fn listen(&self, addr: std::net::SocketAddr, backlog: i32) -> std::io::Result<()>;
    fn accept(
        &self,
        fd: std::os::unix::io::RawFd,
    ) -> std::io::Result<(std::os::unix::io::RawFd, std::net::SocketAddr)>;
    fn send(&self, fd: std::os::unix::io::RawFd, buf: &[u8], flags: i32) -> std::io::Result<usize>;
    fn recv(
        &self,
        fd: std::os::unix::io::RawFd,
        buf: &mut [u8],
        flags: i32,
    ) -> std::io::Result<usize>;
}

pub trait IoUringAdapter: Send + Sync {
    fn submit(&self, sqe: &SyscallSqe) -> std::io::Result<u64>;
    fn wait(&self, timeout_ms: u32) -> std::io::Result<Vec<SyscallCqe>>;
    fn setup(&self, entries: u32, flags: u32) -> std::io::Result<std::os::unix::io::RawFd>;
}

#[repr(C)]
pub struct SyscallSqe {
    pub opcode: u8,
    pub flags: u8,
    pub ioprio: u16,
    pub fd: i32,
    pub off_addr2: u64,
    pub addr: u64,
    pub len: u32,
    pub rw_flags: u32,
    pub user_data: u64,
    pub buf_index: u16,
    pub personality: u16,
    pub splice_fd_in: i32,
    pub addr3: u64,
    pub resv: u64,
}

#[repr(C)]
pub struct SyscallCqe {
    pub res: i32,
    pub flags: u32,
    pub user_data: u64,
}
