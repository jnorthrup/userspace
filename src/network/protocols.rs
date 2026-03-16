//! Network protocol detection and classification

/// Supported network protocols
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Https,
    Http2,
    Http3,
    Quic,
    Ssh,
    Tls,
    WebSocket,
    Raw,
    Unknown,
}

/// Protocol detector for identifying network protocols from byte streams
pub struct ProtocolDetector {
    buffer: Vec<u8>,
    detected: Option<Protocol>,
}

impl ProtocolDetector {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(256),
            detected: None,
        }
    }

    /// Feed bytes to the detector
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        if self.detected.is_none() {
            self.detected = self.detect_protocol();
        }
    }

    /// Get the detected protocol if any
    pub fn protocol(&self) -> Option<Protocol> {
        self.detected
    }

    /// Detect protocol from buffered data
    fn detect_protocol(&self) -> Option<Protocol> {
        if self.buffer.len() < 4 {
            return None;
        }

        // Simple protocol detection based on first bytes
        match &self.buffer[..] {
            // HTTP methods
            b"GET " | b"POST" | b"PUT " | b"HEAD" | b"DELE" => Some(Protocol::Http),
            // TLS handshake
            data if data.len() >= 3 && data[0] == 0x16 && data[1] == 0x03 => Some(Protocol::Tls),
            // SSH banner
            data if data.starts_with(b"SSH-") => Some(Protocol::Ssh),
            // QUIC
            data if data.len() >= 1 && (data[0] & 0xf0) == 0xc0 => Some(Protocol::Quic),
            _ => None,
        }
    }

    /// Reset the detector
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.detected = None;
    }
}

impl Default for ProtocolDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect protocol from a byte slice
pub fn detect_protocol(data: &[u8]) -> Protocol {
    let mut detector = ProtocolDetector::new();
    detector.feed(data);
    detector.protocol().unwrap_or(Protocol::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_detection() {
        let mut detector = ProtocolDetector::new();
        detector.feed(b"GET / HTTP/1.1\r\n");
        assert_eq!(detector.protocol(), Some(Protocol::Http));
    }

    #[test]
    fn test_ssh_detection() {
        let mut detector = ProtocolDetector::new();
        detector.feed(b"SSH-2.0-OpenSSH_8.0\r\n");
        assert_eq!(detector.protocol(), Some(Protocol::Ssh));
    }
}
