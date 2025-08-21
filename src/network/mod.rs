//! Network abstractions and protocol adapters
//!
//! This module provides unified network protocol handling with adapters
//! for HTTP, QUIC, SSH, and other protocols.

#[cfg(feature = "network")]
pub mod adapters;

#[cfg(feature = "network")]
pub mod protocols;

#[cfg(feature = "network")]
pub mod channels;

#[cfg(feature = "network")]
pub use adapters::{NetworkAdapter, AdapterType};

#[cfg(feature = "network")]
pub use protocols::{Protocol, ProtocolDetector};

#[cfg(feature = "network")]
pub use channels::{Channel, ChannelProvider};