//! Knox Proxy - Network proxy for secure tunneling
//!
//! Provides transparent proxy functionality.

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};

pub struct KnoxProxy {
    listener: TcpListener,
    target_addr: SocketAddr,
}

impl KnoxProxy {
    pub fn new(listen_addr: SocketAddr, target_addr: SocketAddr) -> io::Result<Self> {
        let listener = TcpListener::bind(listen_addr)?;
        listener.set_nonblocking(false)?;

        Ok(Self {
            listener,
            target_addr,
        })
    }

    pub fn accept(&self) -> io::Result<(RawFd, SocketAddr)> {
        let (stream, addr) = self.listener.accept()?;
        Ok((stream.into_raw_fd(), addr))
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn target_addr(&self) -> SocketAddr {
        self.target_addr
    }

    pub fn run_loop<F>(self, mut handler: F) -> io::Result<()>
    where
        F: FnMut(RawFd, SocketAddr) -> io::Result<()>,
    {
        for result in self.listener.incoming() {
            match result {
                Ok(stream) => {
                    let fd = stream.into_raw_fd();
                    if let Err(e) = handler(fd, self.target_addr) {
                        eprintln!("handler error: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("accept error: {}", e);
                }
            }
        }
        Ok(())
    }
}

pub struct TunnelSession {
    client_fd: RawFd,
    target_stream: TcpStream,
}

impl TunnelSession {
    pub fn new(client_fd: RawFd, target_addr: SocketAddr) -> io::Result<Self> {
        let target_stream = TcpStream::connect(target_addr)?;
        target_stream.set_nonblocking(false)?;

        Ok(Self {
            client_fd,
            target_stream,
        })
    }

    pub fn transfer(&mut self) -> io::Result<u64> {
        use std::io::{Read, Write};

        let mut buf = [0u8; 65536];
        let mut total = 0u64;
        let mut client_stream = unsafe { TcpStream::from_raw_fd(self.client_fd) };

        loop {
            let n = client_stream.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.target_stream.write_all(&buf[..n])?;
            total += n as u64;

            let n = self.target_stream.read(&mut buf)?;
            if n == 0 {
                break;
            }
            client_stream.write_all(&buf[..n])?;
            total += n as u64;
        }

        Ok(total)
    }

    pub fn close(&self) {
        let stream = unsafe { TcpStream::from_raw_fd(self.client_fd) };
        let _ = stream.shutdown(std::net::Shutdown::Both);
        let _ = self.target_stream.shutdown(std::net::Shutdown::Both);
    }
}

impl Drop for TunnelSession {
    fn drop(&mut self) {
        unsafe { libc::close(self.client_fd) };
    }
}
