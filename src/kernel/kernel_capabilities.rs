//! Kernel Capabilities Detection
//!
//! Provides a unified interface for detecting system capabilities
//! related to kernel features, performance optimizations, and
//! low-level system interfaces.

/// Represents the detected system capabilities
#[derive(Debug, Clone)]
pub struct SystemCapabilities {
    pub io_uring_available: bool,
    pub ebpf_capable: bool,
    pub hugepages_available: bool,
    pub numa_supported: bool,
    pub simd_extensions: Vec<String>,
}

impl SystemCapabilities {
    /// Detect system capabilities
    pub fn detect() -> Self {
        Self {
            io_uring_available: Self::detect_io_uring(),
            ebpf_capable: Self::detect_ebpf(),
            hugepages_available: Self::detect_hugepages(),
            numa_supported: Self::detect_numa(),
            simd_extensions: Self::detect_simd_extensions(),
        }
    }

    /// Check if io_uring is supported
    fn detect_io_uring() -> bool {
        #[cfg(target_os = "linux")]
        {
            // Check kernel support file
            fs::metadata("/proc/sys/kernel/io_uring_disabled").is_ok() &&
            // Check kernel version (requires Linux 5.1+)
            env::var("KERNEL_VERSION")
                .map(|v| v.split('.').collect::<Vec<_>>())
                .ok()
                .and_then(|parts| {
                    if parts.len() >= 2 {
                        parts[0].parse::<u32>().ok().zip(parts[1].parse::<u32>().ok())
                    } else {
                        None
                    }
                })
                .map_or(false, |(major, minor)| major > 5 || (major == 5 && minor >= 1))
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Check if eBPF is supported
    fn detect_ebpf() -> bool {
        #[cfg(target_os = "linux")]
        {
            fs::metadata("/sys/fs/bpf").is_ok()
                && fs::metadata("/proc/sys/net/core/bpf_jit_enable").is_ok()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Check if hugepages are available
    fn detect_hugepages() -> bool {
        #[cfg(target_os = "linux")]
        {
            fs::read_to_string("/proc/meminfo")
                .map(|meminfo| meminfo.contains("HugePages_Total"))
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Check if NUMA is supported
    fn detect_numa() -> bool {
        #[cfg(target_os = "linux")]
        {
            fs::read_to_string("/proc/cpuinfo")
                .map(|cpuinfo| cpuinfo.contains("numa"))
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Detect SIMD extensions
    fn detect_simd_extensions() -> Vec<String> {
        let mut extensions = Vec::new();

        #[cfg(target_arch = "x86_64")]
        {
            #[cfg(target_feature = "sse2")]
            extensions.push("SSE2".to_string());

            #[cfg(target_feature = "sse4.2")]
            extensions.push("SSE4.2".to_string());

            #[cfg(target_feature = "avx")]
            extensions.push("AVX".to_string());

            #[cfg(target_feature = "avx2")]
            extensions.push("AVX2".to_string());

            #[cfg(target_feature = "avx512f")]
            extensions.push("AVX-512F".to_string());
        }

        #[cfg(target_arch = "aarch64")]
        {
            #[cfg(target_feature = "neon")]
            extensions.push("NEON".to_string());
        }

        extensions
    }

    /// Print detected capabilities for debugging
    pub fn print_capabilities(&self) {
        println!("System Capabilities:");
        println!("  io_uring:        {}", self.io_uring_available);
        println!("  eBPF:           {}", self.ebpf_capable);
        println!("  Hugepages:      {}", self.hugepages_available);
        println!("  NUMA:           {}", self.numa_supported);
        println!("  SIMD Extensions: {}", self.simd_extensions.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_capabilities_detection() {
        let caps = SystemCapabilities::detect();
        assert!(
            caps.simd_extensions.len() > 0,
            "No SIMD extensions detected"
        );
    }
}
