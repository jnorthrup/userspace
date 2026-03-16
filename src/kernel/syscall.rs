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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::os::unix::io::RawFd;

    #[test]
    fn test_syscall_sqe_abi_64_bytes() {
        let sqe = SyscallSqe {
            opcode: 0,
            flags: 0,
            ioprio: 0,
            fd: -1,
            off_addr2: 0,
            addr: 0,
            len: 0,
            rw_flags: 0,
            user_data: 0,
            buf_index: 0,
            personality: 0,
            splice_fd_in: 0,
            addr3: 0,
            resv: 0,
        };
        assert_eq!(
            std::mem::size_of::<SyscallSqe>(),
            64,
            "SQE must be 64 bytes for kernel ABI"
        );
        assert_eq!(sqe.fd, -1);
    }

    #[test]
    fn test_syscall_cqe_abi_16_bytes() {
        let cqe = SyscallCqe {
            res: 0,
            flags: 0,
            user_data: 0,
        };
        assert_eq!(
            std::mem::size_of::<SyscallCqe>(),
            16,
            "CQE must be 16 bytes for kernel ABI"
        );
    }

    #[test]
    fn test_syscall_adapter_trait() {
        struct MockAdapter;
        impl SyscallAdapter for MockAdapter {
            fn read(&self, fd: RawFd, buf: &mut [u8]) -> std::io::Result<usize> {
                assert!(fd >= 0);
                Ok(buf.len())
            }
            fn write(&self, fd: RawFd, buf: &[u8]) -> std::io::Result<usize> {
                assert!(fd >= 0);
                Ok(buf.len())
            }
            fn close(&self, fd: RawFd) -> std::io::Result<()> {
                assert!(fd >= 0);
                Ok(())
            }
        }

        let adapter = MockAdapter;
        let mut buf = [0u8; 10];
        assert_eq!(adapter.read(3, &mut buf).unwrap(), 10);
        assert_eq!(adapter.write(3, b"hello").unwrap(), 5);
        adapter.close(3).unwrap();
    }

    #[test]
    fn test_network_adapter_trait() {
        struct MockNetworkAdapter;
        impl NetworkAdapter for MockNetworkAdapter {
            fn connect(&self, addr: SocketAddr) -> std::io::Result<RawFd> {
                let _ = addr;
                Ok(10)
            }
            fn bind(&self, addr: SocketAddr) -> std::io::Result<RawFd> {
                let _ = addr;
                Ok(11)
            }
            fn listen(&self, addr: SocketAddr, backlog: i32) -> std::io::Result<()> {
                let _ = (addr, backlog);
                Ok(())
            }
            fn accept(&self, fd: RawFd) -> std::io::Result<(RawFd, SocketAddr)> {
                let _ = fd;
                Ok((12, "127.0.0.1:8080".parse().unwrap()))
            }
            fn send(&self, fd: RawFd, buf: &[u8], flags: i32) -> std::io::Result<usize> {
                let _ = (fd, flags);
                Ok(buf.len())
            }
            fn recv(&self, fd: RawFd, buf: &mut [u8], flags: i32) -> std::io::Result<usize> {
                let _ = (fd, flags);
                Ok(0)
            }
        }

        let adapter = MockNetworkAdapter;
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

        assert_eq!(adapter.connect(addr).unwrap(), 10);
        assert_eq!(adapter.bind(addr).unwrap(), 11);
        adapter.listen(addr, 128).unwrap();

        let (fd, peer_addr) = adapter.accept(11).unwrap();
        assert_eq!(fd, 12);
        assert_eq!(peer_addr.to_string(), "127.0.0.1:8080");

        assert_eq!(adapter.send(10, b"test", 0).unwrap(), 4);

        let mut buf = [0u8; 10];
        assert_eq!(adapter.recv(10, &mut buf, 0).unwrap(), 0);
    }

    #[test]
    fn test_io_uring_adapter_trait() {
        struct MockIoUringAdapter;
        impl IoUringAdapter for MockIoUringAdapter {
            fn submit(&self, sqe: &SyscallSqe) -> std::io::Result<u64> {
                let _ = sqe;
                Ok(1)
            }
            fn wait(&self, timeout_ms: u32) -> std::io::Result<Vec<SyscallCqe>> {
                let _ = timeout_ms;
                Ok(vec![])
            }
            fn setup(&self, entries: u32, flags: u32) -> std::io::Result<RawFd> {
                let _ = (entries, flags);
                Ok(100)
            }
        }

        let adapter = MockIoUringAdapter;
        let sqe = SyscallSqe {
            opcode: 1,
            flags: 0,
            ioprio: 0,
            fd: 5,
            off_addr2: 0,
            addr: 0,
            len: 100,
            rw_flags: 0,
            user_data: 42,
            buf_index: 0,
            personality: 0,
            splice_fd_in: 0,
            addr3: 0,
            resv: 0,
        };

        assert_eq!(adapter.submit(&sqe).unwrap(), 1);
        assert!(adapter.wait(1000).unwrap().is_empty());
        assert_eq!(adapter.setup(1024, 0).unwrap(), 100);
    }
}
