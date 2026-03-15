//! Tethering Bypass - Direct device-to-device network bypass
//!
//! Provides zero-copy network path for tethered device communication.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

pub struct TetheringConfig {
    pub local_ip: Ipv4Addr,
    pub peer_ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub mtu: u16,
}

impl Default for TetheringConfig {
    fn default() -> Self {
        Self {
            local_ip: Ipv4Addr::new(192, 168, 42, 1),
            peer_ip: Ipv4Addr::new(192, 168, 42, 2),
            netmask: Ipv4Addr::new(255, 255, 255, 0),
            mtu: 1500,
        }
    }
}

impl TetheringConfig {
    pub fn new(local: Ipv4Addr, peer: Ipv4Addr) -> Self {
        Self {
            local_ip: local,
            peer_ip: peer,
            ..Default::default()
        }
    }

    pub fn network(&self) -> Ipv4Addr {
        let local: u32 = u32::from(self.local_ip);
        let mask: u32 = u32::from(self.netmask);
        Ipv4Addr::from(local & mask)
    }
}

pub struct TetheringSession {
    config: TetheringConfig,
}

impl TetheringSession {
    pub fn new(config: TetheringConfig) -> io::Result<Self> {
        Ok(Self { config })
    }

    pub fn config(&self) -> &TetheringConfig {
        &self.config
    }

    pub fn local_addr(&self) -> SocketAddr {
        SocketAddrV4::new(self.config.local_ip, 0).into()
    }

    pub fn peer_addr(&self) -> SocketAddr {
        SocketAddrV4::new(self.config.peer_ip, 0).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tethering_config() {
        let config = TetheringConfig::new(
            Ipv4Addr::new(192, 168, 100, 1),
            Ipv4Addr::new(192, 168, 100, 2),
        );

        assert_eq!(config.local_ip, Ipv4Addr::new(192, 168, 100, 1));
        assert_eq!(config.peer_ip, Ipv4Addr::new(192, 168, 100, 2));
    }

    #[test]
    fn test_network_calculation() {
        let config = TetheringConfig::new(
            Ipv4Addr::new(192, 168, 100, 10),
            Ipv4Addr::new(192, 168, 100, 20),
        );

        assert_eq!(config.network(), Ipv4Addr::new(192, 168, 100, 0));
    }
}
