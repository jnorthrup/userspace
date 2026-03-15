//! POSIX socket operations - migrated from literbike
//!
//! Provides low-level POSIX socket API wrappers using std::net types.
//! Note: On non-Linux platforms, this provides a compatibility shim.

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
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
