//! Network protocol adapters for various transport protocols

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Type of network adapter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterType {
    Http,
    Https,
    Quic,
    Ssh,
    WebSocket,
    Raw,
}

/// Trait for network protocol adapters
pub trait NetworkAdapter: AsyncRead + AsyncWrite + Send + Sync + Unpin {
    /// Get the adapter type
    fn adapter_type(&self) -> AdapterType;
    
    /// Get the remote address
    fn remote_addr(&self) -> io::Result<SocketAddr>;
    
    /// Check if the connection is established
    fn is_connected(&self) -> bool;
    
    /// Close the connection
    fn close(&mut self) -> io::Result<()>;
}

/// Trait combining AsyncRead and AsyncWrite for network streams
pub trait NetworkStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {}

/// Implement NetworkStream for any type that meets the requirements
impl<T> NetworkStream for T where T: AsyncRead + AsyncWrite + Send + Sync + Unpin {}

/// HTTP adapter for HTTP/1.1 and HTTP/2 protocols
pub struct HttpAdapter {
    inner: Box<dyn NetworkStream>,
    remote: SocketAddr,
    connected: bool,
}

impl HttpAdapter {
    pub fn new<T>(inner: T, remote: SocketAddr) -> Self
    where
        T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        Self {
            inner: Box::new(inner),
            remote,
            connected: true,
        }
    }
}

impl NetworkAdapter for HttpAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Http
    }
    
    fn remote_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.remote)
    }
    
    fn is_connected(&self) -> bool {
        self.connected
    }
    
    fn close(&mut self) -> io::Result<()> {
        self.connected = false;
        Ok(())
    }
}

impl AsyncRead for HttpAdapter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for HttpAdapter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.connected = false;
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// QUIC adapter for QUIC protocol
pub struct QuicAdapter {
    inner: Box<dyn NetworkStream>,
    remote: SocketAddr,
    stream_id: u64,
    connected: bool,
}

impl QuicAdapter {
    pub fn new<T>(inner: T, remote: SocketAddr, stream_id: u64) -> Self
    where
        T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        Self {
            inner: Box::new(inner),
            remote,
            stream_id,
            connected: true,
        }
    }
    
    pub fn stream_id(&self) -> u64 {
        self.stream_id
    }
}

impl NetworkAdapter for QuicAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Quic
    }
    
    fn remote_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.remote)
    }
    
    fn is_connected(&self) -> bool {
        self.connected
    }
    
    fn close(&mut self) -> io::Result<()> {
        self.connected = false;
        Ok(())
    }
}

impl AsyncRead for QuicAdapter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for QuicAdapter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.connected = false;
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// SSH adapter for SSH protocol
pub struct SshAdapter {
    inner: Box<dyn NetworkStream>,
    remote: SocketAddr,
    session_id: Vec<u8>,
    connected: bool,
}

impl SshAdapter {
    pub fn new<T>(inner: T, remote: SocketAddr) -> Self
    where
        T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        Self {
            inner: Box::new(inner),
            remote,
            session_id: Vec::new(),
            connected: true,
        }
    }
    
    pub fn set_session_id(&mut self, id: Vec<u8>) {
        self.session_id = id;
    }
    
    pub fn session_id(&self) -> &[u8] {
        &self.session_id
    }
}

impl NetworkAdapter for SshAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Ssh
    }
    
    fn remote_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.remote)
    }
    
    fn is_connected(&self) -> bool {
        self.connected
    }
    
    fn close(&mut self) -> io::Result<()> {
        self.connected = false;
        Ok(())
    }
}

impl AsyncRead for SshAdapter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for SshAdapter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.connected = false;
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Factory for creating network adapters
pub struct AdapterFactory;

impl AdapterFactory {
    /// Create an adapter based on the detected protocol
    pub fn create_adapter(
        adapter_type: AdapterType,
        stream: tokio::net::TcpStream,
        remote: SocketAddr,
    ) -> Box<dyn NetworkAdapter> {
        match adapter_type {
            AdapterType::Http | AdapterType::Https => {
                Box::new(HttpAdapter::new(stream, remote))
            }
            AdapterType::Quic => {
                Box::new(QuicAdapter::new(stream, remote, 0))
            }
            AdapterType::Ssh => {
                Box::new(SshAdapter::new(stream, remote))
            }
            _ => {
                // Default to HTTP adapter for unsupported types
                Box::new(HttpAdapter::new(stream, remote))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    
    #[tokio::test]
    async fn test_http_adapter() {
        let (client, server) = tokio::io::duplex(1024);
        let addr = "127.0.0.1:8080".parse().unwrap();
        let mut adapter = HttpAdapter::new(client, addr);
        
        assert_eq!(adapter.adapter_type(), AdapterType::Http);
        assert!(adapter.is_connected());
        assert_eq!(adapter.remote_addr().unwrap(), addr);
        
        // Test write
        adapter.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
        
        // Test close
        adapter.close().unwrap();
        assert!(!adapter.is_connected());
    }
    
    #[tokio::test]
    async fn test_quic_adapter() {
        let (client, _server) = tokio::io::duplex(1024);
        let addr = "127.0.0.1:4433".parse().unwrap();
        let adapter = QuicAdapter::new(client, addr, 1);
        
        assert_eq!(adapter.adapter_type(), AdapterType::Quic);
        assert_eq!(adapter.stream_id(), 1);
        assert!(adapter.is_connected());
    }
    
    #[tokio::test]
    async fn test_ssh_adapter() {
        let (client, _server) = tokio::io::duplex(1024);
        let addr = "127.0.0.1:22".parse().unwrap();
        let mut adapter = SshAdapter::new(client, addr);
        
        adapter.set_session_id(vec![1, 2, 3, 4]);
        assert_eq!(adapter.adapter_type(), AdapterType::Ssh);
        assert_eq!(adapter.session_id(), &[1, 2, 3, 4]);
        assert!(adapter.is_connected());
    }
}