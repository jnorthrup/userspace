//! Direct syscall network operations - densified from literbike
//!
//! Low-level network operations using direct syscalls for maximum performance.
//! Integrated with ENDGAME kernel bypass for zero-overhead networking.

use crate::kernel::endgame_bypass::DensifiedKernel;
use std::collections::HashMap;
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::ffi::CStr;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::unix::io::RawFd;

/// Network interface for densified operations
#[allow(dead_code)]
pub struct NetworkInterface {
    name: String,
    addrs: Vec<InterfaceAddr>,
    flags: u32,
}

#[derive(Clone, Debug)]
pub enum InterfaceAddr {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

/// Socket operations with kernel bypass integration
pub struct SocketOps {
    densified: Option<DensifiedKernel>,
}

impl SocketOps {
    pub fn new(densified: Option<DensifiedKernel>) -> Self {
        Self { densified }
    }

    /// Direct syscall send with optional kernel bypass
    pub unsafe fn send(&self, fd: RawFd, msg: *const libc::msghdr, flags: i32) -> isize {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            match &self.densified {
                Some(kernel) => kernel.densified_send(fd, msg, flags),
                None => libc::sendmsg(fd, msg, flags),
            }
        }
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = self.densified;
            libc::sendmsg(fd, msg, flags)
        }
    }

    /// Direct syscall recv with optional kernel bypass
    pub unsafe fn recv(&self, fd: RawFd, msg: *mut libc::msghdr, flags: i32) -> isize {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            match &self.densified {
                Some(kernel) => kernel.densified_recv(fd, msg, flags),
                None => libc::recvmsg(fd, msg, flags),
            }
        }
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = self.densified;
            libc::recvmsg(fd, msg, flags)
        }
    }
}

/// Get default gateway using direct kernel file parsing
pub fn get_default_gateway() -> io::Result<Ipv4Addr> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    return parse_proc_net_route();

    #[cfg(target_os = "macos")]
    return parse_netstat_route();

    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
    Err(io::Error::new(io::ErrorKind::Other, "Unsupported OS"))
}

/// Get default IPv6 gateway using direct kernel access
pub fn get_default_gateway_v6() -> io::Result<Ipv6Addr> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    return parse_proc_net_ipv6_route();

    #[cfg(target_os = "macos")]
    return parse_netstat_route_v6();

    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
    Err(io::Error::new(io::ErrorKind::Other, "Unsupported OS"))
}

/// Get default local IPv4 using UDP socket probing
pub fn get_default_local_ipv4() -> io::Result<Ipv4Addr> {
    use std::net::{SocketAddr, UdpSocket};

    let sock = UdpSocket::bind(("0.0.0.0", 0))?;
    let targets = [("1.1.1.1", 80u16), ("8.8.8.8", 80u16), ("9.9.9.9", 80u16)];
    for (host, port) in targets {
        if sock.connect((host, port)).is_ok() {
            if let Ok(local) = sock.local_addr() {
                if let SocketAddr::V4(sa) = local {
                    return Ok(*sa.ip());
                }
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::Other,
        "unable to determine local IPv4",
    ))
}

/// Get default local IPv6 using UDP socket probing
pub fn get_default_local_ipv6() -> io::Result<Ipv6Addr> {
    use std::net::{SocketAddr, UdpSocket};

    let sock = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0))?;
    let targets = [
        ("2001:4860:4860::8888", 80u16),
        ("2606:4700:4700::1111", 80u16),
    ];
    for (host, port) in targets {
        if sock.connect((host, port)).is_ok() {
            if let Ok(local) = sock.local_addr() {
                if let SocketAddr::V6(sa) = local {
                    return Ok(*sa.ip());
                }
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::Other,
        "unable to determine local IPv6",
    ))
}

/// List network interfaces with addresses
pub fn list_interfaces() -> io::Result<HashMap<String, NetworkInterface>> {
    #[allow(unused_mut)]
    let mut interfaces = HashMap::new();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        unsafe {
            let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
            if libc::getifaddrs(&mut ifap) != 0 {
                return Err(io::Error::last_os_error());
            }

            let mut current = ifap;
            while !current.is_null() {
                let ifa = &*current;
                if !ifa.ifa_name.is_null() {
                    let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy().to_string();

                    let entry = interfaces.entry(name.clone()).or_insert(NetworkInterface {
                        name: name.clone(),
                        addrs: Vec::new(),
                        flags: ifa.ifa_flags,
                    });

                    if !ifa.ifa_addr.is_null() {
                        let sa = &*(ifa.ifa_addr as *const libc::sockaddr);
                        match sa.sa_family as i32 {
                            libc::AF_INET => {
                                let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                                let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                                entry.addrs.push(InterfaceAddr::V4(ip));
                            }
                            libc::AF_INET6 => {
                                let sin6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                                let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
                                entry.addrs.push(InterfaceAddr::V6(ip));
                            }
                            _ => {}
                        }
                    }
                }
                current = ifa.ifa_next;
            }

            libc::freeifaddrs(ifap);
        }
    }

    Ok(interfaces)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn parse_proc_net_route() -> io::Result<Ipv4Addr> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open("/proc/net/route")?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 3 && fields[1] == "00000000" {
            if let Ok(gateway) = u32::from_str_radix(fields[2], 16) {
                return Ok(Ipv4Addr::from(gateway.to_le()));
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "No default gateway found",
    ))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn parse_proc_net_ipv6_route() -> io::Result<Ipv6Addr> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open("/proc/net/ipv6_route")?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 5 && fields[0] == "00000000000000000000000000000000" {
            let gateway_hex = &fields[4];
            if gateway_hex != "00000000000000000000000000000000" {
                let mut bytes = [0u8; 16];
                for i in 0..16 {
                    if let Ok(byte) = u8::from_str_radix(&gateway_hex[i * 2..i * 2 + 2], 16) {
                        bytes[i] = byte;
                    }
                }
                return Ok(Ipv6Addr::from(bytes));
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "No default IPv6 gateway found",
    ))
}

#[cfg(target_os = "macos")]
fn parse_netstat_route() -> io::Result<Ipv4Addr> {
    use std::process::Command;

    let output = Command::new("netstat")
        .args(&["-rn", "-f", "inet"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("default") || line.starts_with("0.0.0.0") {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 2 {
                if let Ok(gateway) = fields[1].parse::<Ipv4Addr>() {
                    return Ok(gateway);
                }
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "No default gateway found",
    ))
}

#[cfg(target_os = "macos")]
fn parse_netstat_route_v6() -> io::Result<Ipv6Addr> {
    use std::process::Command;

    let output = Command::new("netstat")
        .args(&["-rn", "-f", "inet6"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("default") || line.starts_with("::") {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 2 {
                if let Ok(gateway) = fields[1].parse::<Ipv6Addr>() {
                    return Ok(gateway);
                }
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "No default IPv6 gateway found",
    ))
}
