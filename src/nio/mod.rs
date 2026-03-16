//! Unified NIO (Non-Blocking I/O) SPI Facade Layer
//!
//! This module provides a public Service Provider Interface (SPI) facade
//! over kernel io_uring and posix_sockets implementations.
//!
//! Exposed facades:
//! - `socket_create()` — syscall wrapper for socket creation
//! - `socket_read()` — non-blocking socket read
//! - `socket_write()` — non-blocking socket write  
//! - `mmap_region()` — memory-mapped region allocation
//! - `io_uring_submit()` — async I/O submission via io_uring

use std::io;
use std::net::SocketAddr;
use std::os::unix::io::RawFd;

#[cfg(all(feature = "kernel", target_os = "linux"))]
use crate::kernel::io_uring;
#[cfg(feature = "syscall-net")]
use crate::kernel::posix_sockets;

/// Result type for NIO operations
pub type NioResult<T> = io::Result<T>;

/// Read from a raw file descriptor
#[cfg(feature = "syscall-net")]
fn read_fd(fd: RawFd, buf: &mut [u8]) -> io::Result<usize> {
    let ret = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret as usize)
    }
}

/// Write to a raw file descriptor
#[cfg(feature = "syscall-net")]
fn write_fd(fd: RawFd, buf: &[u8]) -> io::Result<usize> {
    let ret = unsafe { libc::write(fd, buf.as_ptr() as *const libc::c_void, buf.len()) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret as usize)
    }
}

/// Socket type for creation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    Stream,
    Dgram,
}

/// Domain for socket creation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketDomain {
    Inet,
    Inet6,
    Unix,
}

impl SocketDomain {
    fn to_libc_domain(&self) -> i32 {
        match self {
            SocketDomain::Inet => libc::AF_INET,
            SocketDomain::Inet6 => libc::AF_INET6,
            SocketDomain::Unix => libc::AF_UNIX,
        }
    }
}

/// Handle to a created socket
#[derive(Debug)]
pub struct SocketHandle {
    fd: RawFd,
}

impl SocketHandle {
    pub fn new(fd: RawFd) -> Self {
        Self { fd }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }
}

/// Memory-mapped region handle
#[derive(Debug)]
pub struct MmapRegion {
    ptr: *mut libc::c_void,
    size: usize,
}

impl MmapRegion {
    pub fn new(ptr: *mut libc::c_void, size: usize) -> Self {
        Self { ptr, size }
    }

    pub fn as_ptr(&self) -> *mut libc::c_void {
        self.ptr
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

/// io_uring submission handle
#[derive(Debug)]
pub struct IoUringHandle {
    #[cfg(all(feature = "kernel", target_os = "linux"))]
    inner: io_uring::KernelUring,
}

#[cfg(all(feature = "kernel", target_os = "linux"))]
impl IoUringHandle {
    pub fn new(inner: io_uring::KernelUring) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &io_uring::KernelUring {
        &self.inner
    }
}

/// Create a new socket with the specified domain and type
///
/// # Arguments
/// * `domain` - Socket domain (INET, INET6, Unix)
/// * `socket_type` - Socket type (Stream, Dgram)
///
/// # Returns
/// * `NioResult<SocketHandle>` - Handle to the created socket
///
/// # Example
/// ```rust,ignore
/// let socket = socket_create(SocketDomain::Inet, SocketType::Stream)?;
/// ```
pub fn socket_create(domain: SocketDomain, socket_type: SocketType) -> NioResult<SocketHandle> {
    #[cfg(feature = "syscall-net")]
    {
        match socket_type {
            SocketType::Stream => {
                let posix_socket = posix_sockets::PosixSocket::new_stream(domain.to_libc_domain())?;
                Ok(SocketHandle::new(posix_socket.fd()))
            }
            SocketType::Dgram => {
                let posix_socket = posix_sockets::PosixSocket::new_dgram(domain.to_libc_domain())?;
                Ok(SocketHandle::new(posix_socket.fd()))
            }
        }
    }
    #[cfg(not(feature = "syscall-net"))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "syscall-net feature required for socket_create",
        ))
    }
}

/// Read from a socket into a buffer
///
/// # Arguments
/// * `socket` - Socket handle to read from
/// * `buf` - Buffer to read data into
///
/// # Returns
/// * `NioResult<usize>` - Number of bytes read
///
/// # Example
/// ```rust,ignore
/// let mut buf = vec![0u8; 1024];
/// let bytes_read = socket_read(&socket, &mut buf)?;
/// ```
pub fn socket_read(socket: &SocketHandle, buf: &mut [u8]) -> NioResult<usize> {
    #[cfg(feature = "syscall-net")]
    {
        read_fd(socket.fd(), buf)
    }
    #[cfg(not(feature = "syscall-net"))]
    {
        let _ = socket;
        let _ = buf;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "syscall-net feature required for socket_read",
        ))
    }
}

/// Write data to a socket
///
/// # Arguments
/// * `socket` - Socket handle to write to
/// * `buf` - Buffer containing data to write
///
/// # Returns
/// * `NioResult<usize>` - Number of bytes written
///
/// # Example
/// ```rust,ignore
/// let data = b"Hello, World!";
/// let bytes_written = socket_write(&socket, data)?;
/// ```
pub fn socket_write(socket: &SocketHandle, buf: &[u8]) -> NioResult<usize> {
    #[cfg(feature = "syscall-net")]
    {
        write_fd(socket.fd(), buf)
    }
    #[cfg(not(feature = "syscall-net"))]
    {
        let _ = socket;
        let _ = buf;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "syscall-net feature required for socket_write",
        ))
    }
}

/// Create a memory-mapped region
///
/// # Arguments
/// * `size` - Size of the region in bytes
///
/// # Returns
/// * `NioResult<MmapRegion>` - Handle to the mapped region
///
/// # Example
/// ```rust,ignore
/// let region = mmap_region(4096)?;
/// ```
pub fn mmap_region(size: usize) -> NioResult<MmapRegion> {
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };

    if ptr == libc::MAP_FAILED {
        Err(io::Error::last_os_error())
    } else {
        Ok(MmapRegion::new(ptr, size))
    }
}

/// Submit an operation to io_uring
///
/// # Arguments
/// * `ring` - io_uring handle
/// * `op` - Operation string ("read", "write", "recv", "send")
/// * `data` - Data buffer for the operation
///
/// # Returns
/// * `NioResult<()>` - Success or failure
///
/// # Example
/// ```rust,ignore
/// io_uring_submit(&ring, "read", &buffer)?;
/// ```
pub fn io_uring_submit(
    #[cfg(all(feature = "kernel", target_os = "linux"))] ring: &IoUringHandle,
    #[cfg(not(all(feature = "kernel", target_os = "linux")))] _ring: &IoUringHandle,
    op: &str,
    data: &[u8],
) -> NioResult<()> {
    #[cfg(all(feature = "kernel", target_os = "linux"))]
    {
        ring.inner().kernel_dispatch(op, data)
    }
    #[cfg(not(all(feature = "kernel", target_os = "linux")))]
    {
        let _ = op;
        let _ = data;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "kernel + linux feature required for io_uring_submit",
        ))
    }
}

/// Initialize a new io_uring instance
///
/// # Arguments
/// * `entries` - Number of queue entries
///
/// # Returns
/// * `NioResult<IoUringHandle>` - Handle to the io_uring instance
pub fn io_uring_init(entries: u32) -> NioResult<IoUringHandle> {
    #[cfg(all(feature = "kernel", target_os = "linux"))]
    {
        let ring = io_uring::KernelUring::new(entries)?;
        Ok(IoUringHandle::new(ring))
    }
    #[cfg(not(all(feature = "kernel", target_os = "linux")))]
    {
        let _ = entries;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "kernel + linux feature required for io_uring_init",
        ))
    }
}

/// Connect a socket to a remote address
///
/// # Arguments
/// * `socket` - Socket handle
/// * `addr` - Remote address to connect to
///
/// # Returns
/// * `NioResult<()>` - Success or failure
pub fn socket_connect(socket: &SocketHandle, addr: SocketAddr) -> NioResult<()> {
    #[cfg(feature = "syscall-net")]
    {
        let addr_bytes = match addr {
            SocketAddr::V4(v4) => {
                let octets = v4.ip().octets();
                let port = v4.port().to_be();
                let mut bytes = vec![libc::AF_INET as u8, 0, 0, 0];
                bytes.extend_from_slice(&port.to_be_bytes());
                bytes.extend_from_slice(&octets);
                bytes
            }
            SocketAddr::V6(v6) => {
                let octets = v6.ip().octets();
                let port = v6.port().to_be();
                let mut bytes = vec![libc::AF_INET6 as u8, 0, 0, 0];
                bytes.extend_from_slice(&port.to_be_bytes());
                bytes.extend_from_slice(&octets);
                bytes
            }
        };
        let ret = unsafe {
            libc::connect(
                socket.fd(),
                addr_bytes.as_ptr() as *const libc::sockaddr,
                addr_bytes.len() as libc::socklen_t,
            )
        };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(feature = "syscall-net"))]
    {
        let _ = socket;
        let _ = addr;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "syscall-net feature required for socket_connect",
        ))
    }
}

/// Bind a socket to a local address
///
/// # Arguments
/// * `socket` - Socket handle
/// * `addr` - Local address to bind to
///
/// # Returns
/// * `NioResult<()>` - Success or failure
pub fn socket_bind(socket: &SocketHandle, addr: SocketAddr) -> NioResult<()> {
    #[cfg(feature = "syscall-net")]
    {
        let addr_bytes = match addr {
            SocketAddr::V4(v4) => {
                let octets = v4.ip().octets();
                let port = v4.port().to_be();
                let mut bytes = vec![libc::AF_INET as u8, 0, 0, 0];
                bytes.extend_from_slice(&port.to_be_bytes());
                bytes.extend_from_slice(&octets);
                bytes
            }
            SocketAddr::V6(v6) => {
                let octets = v6.ip().octets();
                let port = v6.port().to_be();
                let mut bytes = vec![libc::AF_INET6 as u8, 0, 0, 0];
                bytes.extend_from_slice(&port.to_be_bytes());
                bytes.extend_from_slice(&octets);
                bytes
            }
        };
        let ret = unsafe {
            libc::bind(
                socket.fd(),
                addr_bytes.as_ptr() as *const libc::sockaddr,
                addr_bytes.len() as libc::socklen_t,
            )
        };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(feature = "syscall-net"))]
    {
        let _ = socket;
        let _ = addr;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "syscall-net feature required for socket_bind",
        ))
    }
}

/// Listen for incoming connections on a socket
///
/// # Arguments
/// * `socket` - Socket handle
/// * `backlog` - Maximum number of pending connections
///
/// # Returns
/// * `NioResult<()>` - Success or failure
pub fn socket_listen(socket: &SocketHandle, backlog: i32) -> NioResult<()> {
    #[cfg(feature = "syscall-net")]
    {
        let ret = unsafe { libc::listen(socket.fd(), backlog) };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(feature = "syscall-net"))]
    {
        let _ = socket;
        let _ = backlog;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "syscall-net feature required for socket_listen",
        ))
    }
}

/// Accept an incoming connection
///
/// # Arguments
/// * `socket` - Listening socket handle
///
/// # Returns
/// * `NioResult<(SocketHandle, SocketAddr)>` - New socket handle and remote address
pub fn socket_accept(socket: &SocketHandle) -> NioResult<(SocketHandle, SocketAddr)> {
    #[cfg(feature = "syscall-net")]
    {
        use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
        let mut addr_storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        let mut addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        let fd = unsafe {
            libc::accept(
                socket.fd(),
                &mut addr_storage as *mut _ as *mut libc::sockaddr,
                &mut addr_len,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let addr = match addr_storage.ss_family as i32 {
            libc::AF_INET => {
                let sin = unsafe { &*(&addr_storage as *const _ as *const libc::sockaddr_in) };
                let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                let port = u16::from_be(sin.sin_port);
                SocketAddr::V4(SocketAddrV4::new(ip, port))
            }
            libc::AF_INET6 => {
                let sin6 = unsafe { &*(&addr_storage as *const _ as *const libc::sockaddr_in6) };
                let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
                let port = u16::from_be(sin6.sin6_port);
                SocketAddr::V6(SocketAddrV6::new(ip, port, 0, 0))
            }
            _ => {
                unsafe { libc::close(fd) };
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unknown address family",
                ));
            }
        };

        Ok((SocketHandle::new(fd), addr))
    }
    #[cfg(not(feature = "syscall-net"))]
    {
        let _ = socket;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "syscall-net feature required for socket_accept",
        ))
    }
}

/// Set socket to non-blocking mode
///
/// # Arguments
/// * `socket` - Socket handle
/// * `nonblocking` - Whether to enable non-blocking mode
///
/// # Returns
/// * `NioResult<()>` - Success or failure
pub fn socket_set_nonblocking(socket: &SocketHandle, nonblocking: bool) -> NioResult<()> {
    #[cfg(feature = "syscall-net")]
    {
        let flags = unsafe { libc::fcntl(socket.fd(), libc::F_GETFL, 0) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        let new_flags = if nonblocking {
            flags | libc::O_NONBLOCK
        } else {
            flags & !libc::O_NONBLOCK
        };
        let ret = unsafe { libc::fcntl(socket.fd(), libc::F_SETFL, new_flags) };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(feature = "syscall-net"))]
    {
        let _ = socket;
        let _ = nonblocking;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "syscall-net feature required for socket_set_nonblocking",
        ))
    }
}

impl Drop for MmapRegion {
    fn drop(&mut self) {
        if !self.ptr.is_null() && self.size > 0 {
            unsafe {
                libc::munmap(self.ptr, self.size);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_domain_to_libc() {
        assert_eq!(SocketDomain::Inet.to_libc_domain(), libc::AF_INET);
        assert_eq!(SocketDomain::Inet6.to_libc_domain(), libc::AF_INET6);
        assert_eq!(SocketDomain::Unix.to_libc_domain(), libc::AF_UNIX);
    }

    #[test]
    fn test_socket_handle_creation() {
        let handle = SocketHandle::new(42);
        assert_eq!(handle.fd(), 42);
    }

    #[test]
    fn test_mmap_region_creation() {
        let ptr = 0x1000 as *mut libc::c_void;
        let region = MmapRegion::new(ptr, 4096);
        assert_eq!(region.as_ptr(), ptr);
        assert_eq!(region.size(), 4096);
    }

    #[cfg(all(feature = "syscall-net", any(target_os = "macos", target_os = "linux")))]
    #[test]
    fn test_socket_create_stream() {
        let result = socket_create(SocketDomain::Inet, SocketType::Stream);
        if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            assert!(
                result.is_ok(),
                "socket_create should succeed on Unix systems"
            );
        }
    }

    #[cfg(all(feature = "syscall-net", any(target_os = "macos", target_os = "linux")))]
    #[test]
    fn test_socket_create_dgram() {
        let result = socket_create(SocketDomain::Inet, SocketType::Dgram);
        if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            assert!(
                result.is_ok(),
                "socket_create dgram should succeed on Unix systems"
            );
        }
    }

    #[test]
    fn test_mmap_region_success() {
        let result = mmap_region(4096);
        assert!(result.is_ok(), "mmap_region should succeed");
        let region = result.unwrap();
        assert!(!region.as_ptr().is_null());
        assert_eq!(region.size(), 4096);
    }

    #[test]
    fn test_mmap_region_zero_size() {
        let result = mmap_region(0);
        assert!(result.is_err(), "mmap_region with 0 size should fail");
    }
}
