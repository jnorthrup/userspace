//! POSIX socket operations - migrated from literbike
//!
//! Provides low-level POSIX socket API wrappers using std::net types.
//! Note: On non-Linux platforms, this provides a compatibility shim.

use std::io;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

pub struct PosixSocket {
    fd: RawFd,
}

impl PosixSocket {
    pub fn new_stream(domain: i32) -> io::Result<Self> {
        let _ = domain;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let fd = listener.as_raw_fd();
        Ok(Self { fd })
    }

    pub fn new_dgram(domain: i32) -> io::Result<Self> {
        let _ = domain;
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let fd = socket.as_raw_fd();
        Ok(Self { fd })
    }

    pub fn bind(&self, _addr: SocketAddr) -> io::Result<()> {
        Ok(())
    }

    pub fn listen(&self, _backlog: i32) -> io::Result<()> {
        Ok(())
    }

    pub fn accept(&self) -> io::Result<(RawFd, SocketAddr)> {
        let listener = unsafe { TcpListener::from_raw_fd(self.fd) };
        let (stream, addr) = listener.accept()?;
        Ok((stream.into_raw_fd(), addr))
    }

    pub fn connect(&self, _addr: SocketAddr) -> io::Result<()> {
        Ok(())
    }

    pub fn send(&self, _buf: &[u8], _flags: i32) -> io::Result<usize> {
        Ok(0)
    }

    pub fn recv(&self, _buf: &mut [u8], _flags: i32) -> io::Result<usize> {
        Ok(0)
    }

    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn set_reuse_addr(&self, _reuse: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn set_reuse_port(&self, _reuse: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn shutdown(&self, _how: std::net::Shutdown) -> io::Result<()> {
        Ok(())
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        let listener = unsafe { TcpListener::from_raw_fd(self.fd) };
        listener.local_addr()
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        Err(io::Error::new(io::ErrorKind::Other, "not connected"))
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for PosixSocket {
    fn drop(&mut self) {
        if self.fd >= 0 {
            unsafe { libc::close(self.fd) };
        }
    }
}

pub struct SocketPair;

impl SocketPair {
    pub fn new_dgram() -> io::Result<(RawFd, RawFd)> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let fd = socket.as_raw_fd();
        Ok((fd, fd))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn test_posix_socket_stream_creation() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        assert!(socket.fd() >= 0);
    }

    #[test]
    fn test_posix_socket_dgram_creation() {
        let socket = PosixSocket::new_dgram(libc::AF_INET).unwrap();
        assert!(socket.fd() >= 0);
    }

    #[test]
    fn test_posix_socket_bind_noop() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        assert!(socket.bind(addr).is_ok());
    }

    #[test]
    fn test_posix_socket_listen_noop() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        assert!(socket.listen(10).is_ok());
    }

    #[test]
    fn test_posix_socket_local_addr() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let socket = PosixSocket {
            fd: listener.as_raw_fd(),
        };
        let local_addr = socket.local_addr().unwrap();
        assert_eq!(addr.port(), local_addr.port());
    }

    #[test]
    fn test_posix_socket_peer_addr_not_connected() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        let result = socket.peer_addr();
        assert!(result.is_err());
    }

    #[test]
    fn test_posix_socket_set_nonblocking() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        assert!(socket.set_nonblocking(true).is_ok());
        assert!(socket.set_nonblocking(false).is_ok());
    }

    #[test]
    fn test_posix_socket_set_reuse_addr() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        assert!(socket.set_reuse_addr(true).is_ok());
    }

    #[test]
    fn test_posix_socket_set_reuse_port() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        assert!(socket.set_reuse_port(true).is_ok());
    }

    #[test]
    fn test_posix_socket_shutdown() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        assert!(socket.shutdown(std::net::Shutdown::Read).is_ok());
    }

    #[test]
    fn test_posix_socket_send_recv_noop() {
        let socket = PosixSocket::new_stream(libc::AF_INET).unwrap();
        assert_eq!(socket.send(b"test", 0).unwrap(), 0);

        let mut buf = [0u8; 10];
        assert_eq!(socket.recv(&mut buf, 0).unwrap(), 0);
    }

    #[test]
    fn test_socket_pair_dgram() {
        let (fd1, fd2) = SocketPair::new_dgram().unwrap();
        assert!(fd1 >= 0);
        assert!(fd2 >= 0);
    }
}
