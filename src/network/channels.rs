//! Network channel abstractions for unified I/O operations

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Trait for network channels that support async I/O
pub trait Channel: AsyncRead + AsyncWrite + Send + Sync + Unpin {
    /// Get a descriptive name for this channel type
    fn channel_type(&self) -> &str;
    
    /// Check if the channel is still connected
    fn is_connected(&self) -> bool;
    
    /// Get channel metadata if available
    fn metadata(&self) -> Option<ChannelMetadata> {
        None
    }
}

/// Metadata about a channel
#[derive(Debug, Clone)]
pub struct ChannelMetadata {
    pub remote_addr: Option<std::net::SocketAddr>,
    pub local_addr: Option<std::net::SocketAddr>,
    pub protocol: Option<super::protocols::Protocol>,
    pub bytes_read: u64,
    pub bytes_written: u64,
}

impl Default for ChannelMetadata {
    fn default() -> Self {
        Self {
            remote_addr: None,
            local_addr: None,
            protocol: None,
            bytes_read: 0,
            bytes_written: 0,
        }
    }
}

/// Provider for creating channels
pub trait ChannelProvider: Send + Sync {
    /// Create a new channel to the specified address
    fn create_channel(&self, addr: &str) -> io::Result<Box<dyn Channel>>;
    
    /// Get the provider name
    fn provider_name(&self) -> &str;
}

/// Basic TCP channel implementation
pub struct TcpChannel {
    stream: tokio::net::TcpStream,
    metadata: ChannelMetadata,
}

impl TcpChannel {
    pub fn new(stream: tokio::net::TcpStream) -> io::Result<Self> {
        let remote_addr = stream.peer_addr().ok();
        let local_addr = stream.local_addr().ok();
        
        Ok(Self {
            stream,
            metadata: ChannelMetadata {
                remote_addr,
                local_addr,
                ..Default::default()
            },
        })
    }
}

impl Channel for TcpChannel {
    fn channel_type(&self) -> &str {
        "TCP"
    }
    
    fn is_connected(&self) -> bool {
        // Simple check - in production would be more sophisticated
        true
    }
    
    fn metadata(&self) -> Option<ChannelMetadata> {
        Some(self.metadata.clone())
    }
}

impl AsyncRead for TcpChannel {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.stream).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            self.metadata.bytes_read += buf.filled().len() as u64;
        }
        result
    }
}

impl AsyncWrite for TcpChannel {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.stream).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &result {
            self.metadata.bytes_written += *n as u64;
        }
        result
    }
    
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// Default TCP channel provider
pub struct TcpChannelProvider;

impl ChannelProvider for TcpChannelProvider {
    fn create_channel(&self, addr: &str) -> io::Result<Box<dyn Channel>> {
        // In a real implementation, this would be async and create the connection
        Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "Synchronous channel creation not implemented",
        ))
    }
    
    fn provider_name(&self) -> &str {
        "TCP"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_channel_metadata() {
        let metadata = ChannelMetadata::default();
        assert_eq!(metadata.bytes_read, 0);
        assert_eq!(metadata.bytes_written, 0);
    }
    
    #[test]
    fn test_tcp_provider() {
        let provider = TcpChannelProvider;
        assert_eq!(provider.provider_name(), "TCP");
    }
}