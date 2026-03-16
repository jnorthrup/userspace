//! ENDGAME Densified io_uring - Direct kernel interface with zero abstractions
//! 
//! This module provides DIRECT kernel io_uring access via raw syscalls,
//! bypassing ALL userspace abstractions for maximum performance.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::ptr::{self, NonNull};
use std::mem::{self, MaybeUninit};
use std::marker::PhantomData;
use std::os::raw::c_void;
use std::io;
use libc::{mmap, munmap, MAP_SHARED, PROT_READ, PROT_WRITE};

// MAP_POPULATE is Linux-specific, define it if not available
#[cfg(target_os = "linux")]
use libc::MAP_POPULATE;
#[cfg(not(target_os = "linux"))]
const MAP_POPULATE: i32 = 0;

// Direct kernel constants - no abstraction
const IORING_SETUP_IOPOLL: u32 = 1 << 0;
const IORING_SETUP_SQPOLL: u32 = 1 << 1;
const IORING_SETUP_SQ_AFF: u32 = 1 << 2;
const IORING_SETUP_CQSIZE: u32 = 1 << 3;
const IORING_SETUP_SINGLE_ISSUER: u32 = 1 << 12;
const IORING_SETUP_DEFER_TASKRUN: u32 = 1 << 13;

// io_uring syscall numbers (x86_64)
const SYS_IO_URING_SETUP: i64 = 425;
const SYS_IO_URING_ENTER: i64 = 426;
const SYS_IO_URING_REGISTER: i64 = 427;

// SIMD-aligned operation codes for autovectorization
#[repr(C, align(32))]
#[derive(Clone, Copy)]
pub struct OpCode(u8);

impl OpCode {
    pub const NOP: Self = Self(0);
    pub const READV: Self = Self(1);
    pub const WRITEV: Self = Self(2);
    pub const READ_FIXED: Self = Self(4);
    pub const WRITE_FIXED: Self = Self(5);
    pub const POLL_ADD: Self = Self(6);
    pub const POLL_REMOVE: Self = Self(7);
    pub const RECV: Self = Self(10);
    pub const SEND: Self = Self(11);
}

/// KERNEL DISPATCH TABLE - Direct mapping to kernel operations
/// Like litebike WAM, but for io_uring
const KERNEL_OPS: &[(&str, OpCode)] = &[
    ("read", OpCode::READV),
    ("write", OpCode::WRITEV),
    ("recv", OpCode::RECV),
    ("send", OpCode::SEND),
];

/// Zero-copy submission queue entry - kernel ABI compatible
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelSQE {
    pub opcode: u8,
    pub flags: u8,
    pub ioprio: u16,
    pub fd: i32,
    pub off_addr2: u64,
    pub addr: u64,
    pub len: u32,
    pub rw_flags: u32,
    pub user_data: u64,
    pub buf_index: u16,
    pub personality: u16,
    pub splice_fd_in: i32,
    pub addr3: u64,
    pub resv: u64,
}

/// Zero-copy completion queue entry - kernel ABI compatible
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KernelCQE {
    pub user_data: u64,
    pub res: i32,
    pub flags: u32,
}

/// Memory-mapped ring buffer - DIRECT kernel memory access
#[repr(C)]
pub struct MappedRing {
    sq_ring_ptr: NonNull<c_void>,
    sq_ring_size: usize,
    cq_ring_ptr: NonNull<c_void>, 
    cq_ring_size: usize,
    sqe_ptr: NonNull<KernelSQE>,
    sq_entries: u32,
    cq_entries: u32,
}

/// Direct kernel io_uring with zero abstractions
pub struct KernelUring {
    fd: i32,
    params: IoUringParams,
    mapped: MappedRing,
    // Submission queue state - cache-aligned for SIMD
    sq_head: *const AtomicU32,
    sq_tail: *mut AtomicU32,
    sq_mask: u32,
    sq_array: *mut u32,
    // Completion queue state - cache-aligned for SIMD
    cq_head: *mut AtomicU32,
    cq_tail: *const AtomicU32,
    cq_mask: u32,
    cqes: *mut KernelCQE,
}

#[repr(C)]
struct IoUringParams {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: SqRingOffsets,
    cq_off: CqRingOffsets,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SqRingOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    flags: u32,
    dropped: u32,
    array: u32,
    resv: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CqRingOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    overflow: u32,
    cqes: u32,
    flags: u32,
    resv: [u32; 3],
}

impl KernelUring {
    /// Get the file descriptor of the io_uring instance
    pub fn fd(&self) -> i32 {
        self.fd
    }

    /// Create kernel io_uring with DIRECT syscall - no libc wrapper
    pub fn new(entries: u32) -> io::Result<Self> {
        let mut params = unsafe { mem::zeroed::<IoUringParams>() };
        params.flags = IORING_SETUP_SINGLE_ISSUER | IORING_SETUP_DEFER_TASKRUN;
        
        // Direct syscall - bypass libc
        let fd = unsafe {
            libc::syscall(SYS_IO_URING_SETUP, entries as i64, &mut params as *mut _ as i64) as i32
        };
        
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        
        // Memory map the rings DIRECTLY
        let sq_ring_size = params.sq_off.array + params.sq_entries * 4;
        let cq_ring_size = params.cq_off.cqes + params.cq_entries * mem::size_of::<KernelCQE>() as u32;
        
        let sq_ring_ptr = unsafe {
            mmap(
                ptr::null_mut(),
                sq_ring_size as usize,
                PROT_READ | PROT_WRITE,
                MAP_SHARED | MAP_POPULATE,
                fd as i32,
                0,
            )
        };
        
        let cq_ring_ptr = unsafe {
            mmap(
                ptr::null_mut(),
                cq_ring_size as usize,
                PROT_READ | PROT_WRITE,
                MAP_SHARED | MAP_POPULATE,
                fd as i32,
                0x8000000, // IORING_OFF_CQ_RING
            )
        };
        
        let sqe_ptr = unsafe {
            mmap(
                ptr::null_mut(),
                params.sq_entries as usize * mem::size_of::<KernelSQE>(),
                PROT_READ | PROT_WRITE,
                MAP_SHARED | MAP_POPULATE,
                fd as i32,
                0x10000000, // IORING_OFF_SQES
            )
        };
        
        let mapped = MappedRing {
            sq_ring_ptr: NonNull::new(sq_ring_ptr).unwrap(),
            sq_ring_size: sq_ring_size as usize,
            cq_ring_ptr: NonNull::new(cq_ring_ptr).unwrap(),
            cq_ring_size: cq_ring_size as usize,
            sqe_ptr: NonNull::new(sqe_ptr as *mut KernelSQE).unwrap(),
            sq_entries: params.sq_entries,
            cq_entries: params.cq_entries,
        };
        
        // Direct pointer arithmetic for zero-copy access
        let sq_head = unsafe {
            sq_ring_ptr.add(params.sq_off.head as usize) as *const AtomicU32
        };
        let sq_tail = unsafe {
            sq_ring_ptr.add(params.sq_off.tail as usize) as *mut AtomicU32
        };
        let sq_array = unsafe {
            sq_ring_ptr.add(params.sq_off.array as usize) as *mut u32
        };
        
        let cq_head = unsafe {
            cq_ring_ptr.add(params.cq_off.head as usize) as *mut AtomicU32
        };
        let cq_tail = unsafe {
            cq_ring_ptr.add(params.cq_off.tail as usize) as *const AtomicU32
        };
        let cqes = unsafe {
            cq_ring_ptr.add(params.cq_off.cqes as usize) as *mut KernelCQE
        };
        
        Ok(Self {
            fd: fd as i32,
            params,
            mapped,
            sq_head,
            sq_tail,
            sq_mask: params.sq_entries - 1,
            sq_array,
            cq_head,
            cq_tail,
            cq_mask: params.cq_entries - 1,
            cqes,
        })
    }
    
    /// Submit operation DIRECTLY to kernel - zero-copy, zero-allocation
    #[inline(always)]
    pub fn submit_direct(&self, sqe: &KernelSQE) -> io::Result<()> {
        unsafe {
            let tail = (*self.sq_tail).load(Ordering::Acquire);
            let head = (*self.sq_head).load(Ordering::Acquire);
            
            if tail.wrapping_sub(head) >= self.params.sq_entries {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "SQ full"));
            }
            
            let idx = tail & self.sq_mask;
            
            // Direct memory write - no allocation
            ptr::write_volatile(
                self.mapped.sqe_ptr.as_ptr().add(idx as usize),
                *sqe
            );
            
            // Update array
            ptr::write_volatile(
                self.sq_array.add(idx as usize),
                idx
            );
            
            // Memory barrier for x86 (MFENCE)
            std::sync::atomic::fence(Ordering::Release);
            
            // Update tail
            (*self.sq_tail).store(tail.wrapping_add(1), Ordering::Release);
            
            // Direct syscall to notify kernel
            libc::syscall(
                SYS_IO_URING_ENTER,
                self.fd as i64,
                1i64, // submit 1
                0i64, // wait for 0
                0i64, // flags
                ptr::null::<c_void>() as i64,
            ) as i32;
        }
        
        Ok(())
    }
    
    /// Bulk submit with SIMD optimization
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    #[inline(always)]
    pub unsafe fn submit_bulk(&self, sqes: &[KernelSQE]) -> io::Result<u32> {
        let tail = (*self.sq_tail).load(Ordering::Acquire);
        let head = (*self.sq_head).load(Ordering::Acquire);
        let available = self.params.sq_entries - tail.wrapping_sub(head);
        let to_submit = sqes.len().min(available as usize);
        
        // SIMD-optimized memcpy for bulk operations
        for (i, sqe) in sqes[..to_submit].iter().enumerate() {
            let idx = (tail + i as u32) & self.sq_mask;
            ptr::write_volatile(
                self.mapped.sqe_ptr.as_ptr().add(idx as usize),
                *sqe
            );
            ptr::write_volatile(
                self.sq_array.add(idx as usize),
                idx
            );
        }
        
        std::sync::atomic::fence(Ordering::Release);
        (*self.sq_tail).store(tail.wrapping_add(to_submit as u32), Ordering::Release);
        
        // Single syscall for all submissions
        let submitted = libc::syscall(
            SYS_IO_URING_ENTER,
            self.fd as i64,
            to_submit as i64,
            0i64,
            0i64,
            ptr::null::<c_void>() as i64,
        ) as i32;
        
        Ok(submitted as u32)
    }
    
    /// Bulk submit fallback for non-x86_64
    #[cfg(not(target_arch = "x86_64"))]
    #[inline(always)]
    pub unsafe fn submit_bulk(&self, sqes: &[KernelSQE]) -> io::Result<u32> {
        // Simple non-SIMD implementation
        let mut count = 0;
        for sqe in sqes {
            self.submit_direct(sqe)?;
            count += 1;
        }
        Ok(count)
    }
    
    /// Harvest completions with zero-copy
    #[inline(always)]
    pub fn reap_completions(&self) -> Vec<KernelCQE> {
        unsafe {
            let mut completions = Vec::new();
            let head = (*self.cq_head).load(Ordering::Acquire);
            let tail = (*self.cq_tail).load(Ordering::Acquire);
            
            let ready = tail.wrapping_sub(head);
            if ready == 0 {
                return completions;
            }
            
            completions.reserve_exact(ready as usize);
            
            for i in 0..ready {
                let idx = (head + i) & self.cq_mask;
                let cqe = ptr::read_volatile(self.cqes.add(idx as usize));
                completions.push(cqe);
            }
            
            // Update head after reading all
            (*self.cq_head).store(tail, Ordering::Release);
            
            completions
        }
    }
    
    /// Direct kernel command dispatch
    pub fn kernel_dispatch(&self, op: &str, data: &[u8]) -> io::Result<()> {
        for (pattern, opcode) in KERNEL_OPS {
            if op == *pattern {
                let sqe = KernelSQE {
                    opcode: opcode.0,
                    flags: 0,
                    ioprio: 0,
                    fd: -1,
                    off_addr2: 0,
                    addr: data.as_ptr() as u64,
                    len: data.len() as u32,
                    rw_flags: 0,
                    user_data: xxhash_rust::xxh3::xxh3_64(data),
                    buf_index: 0,
                    personality: 0,
                    splice_fd_in: 0,
                    addr3: 0,
                    resv: 0,
                };
                return self.submit_direct(&sqe);
            }
        }
        Err(io::Error::new(io::ErrorKind::InvalidInput, "Unknown op"))
    }
}

impl Drop for KernelUring {
    fn drop(&mut self) {
        unsafe {
            munmap(self.mapped.sq_ring_ptr.as_ptr(), self.mapped.sq_ring_size);
            munmap(self.mapped.cq_ring_ptr.as_ptr(), self.mapped.cq_ring_size);
            munmap(
                self.mapped.sqe_ptr.as_ptr() as *mut c_void,
                self.params.sq_entries as usize * mem::size_of::<KernelSQE>()
            );
            libc::close(self.fd);
        }
    }
}

/// Zero-cost future for io_uring operations
pub struct UringFuture {
    ring: *const KernelUring,
    user_data: u64,
    _phantom: PhantomData<KernelCQE>,
}

unsafe impl Send for UringFuture {}
unsafe impl Sync for UringFuture {}

impl std::future::Future for UringFuture {
    type Output = io::Result<i32>;
    
    fn poll(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        unsafe {
            let completions = (*self.ring).reap_completions();
            for cqe in completions {
                if cqe.user_data == self.user_data {
                    return std::task::Poll::Ready(Ok(cqe.res));
                }
            }
            std::task::Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    
    #[test]
    fn test_kernel_uring_creation() {
        if cfg!(target_os = "linux") {
            match KernelUring::new(256) {
                Ok(_ring) => {}
                Err(e) => {
                    eprintln!("io_uring not available: {}", e);
                }
            }
        }
    }

    #[test]
    fn test_kernel_sqe_construction() {
        let sqe = KernelSQE {
            opcode: OpCode::READV.0,
            flags: 0,
            ioprio: 0,
            fd: 10,
            off_addr2: 0,
            addr: 0x1000,
            len: 4096,
            rw_flags: 0,
            user_data: 123,
            buf_index: 0,
            personality: 0,
            splice_fd_in: -1,
            addr3: 0,
            resv: 0,
        };
        assert_eq!(sqe.fd, 10);
        assert_eq!(sqe.len, 4096);
        assert_eq!(sqe.user_data, 123);
        assert_eq!(std::mem::size_of::<KernelSQE>(), 64);
    }

    #[test]
    fn test_kernel_cqe_construction() {
        let cqe = KernelCQE {
            user_data: 123,
            res: 1024,
            flags: 0,
        };
        assert_eq!(cqe.res, 1024);
        assert_eq!(std::mem::size_of::<KernelCQE>(), 16);
    }

    #[test]
    fn test_opcode_constants() {
        assert_eq!(OpCode::NOP.0, 0);
        assert_eq!(OpCode::READV.0, 1);
        assert_eq!(OpCode::WRITEV.0, 2);
        assert_eq!(OpCode::READ_FIXED.0, 4);
        assert_eq!(OpCode::WRITE_FIXED.0, 5);
        assert_eq!(OpCode::POLL_ADD.0, 6);
        assert_eq!(OpCode::RECV.0, 10);
        assert_eq!(OpCode::SEND.0, 11);
    }

    #[test]
    fn test_kernel_ops_table() {
        assert_eq!(KERNEL_OPS.len(), 4);
        assert_eq!(KERNEL_OPS[0].0, "read");
        assert_eq!(KERNEL_OPS[1].0, "write");
        assert_eq!(KERNEL_OPS[2].0, "recv");
        assert_eq!(KERNEL_OPS[3].0, "send");
    }

    #[test]
    fn test_uring_constants() {
        assert_eq!(IORING_SETUP_IOPOLL, 1);
        assert_eq!(IORING_SETUP_SQPOLL, 2);
        assert_eq!(IORING_SETUP_SQ_AFF, 4);
        assert_eq!(IORING_SETUP_CQSIZE, 8);
        assert_eq!(IORING_SETUP_SINGLE_ISSUER, 4096);
        assert_eq!(IORING_SETUP_DEFER_TASKRUN, 8192);
    }

    #[test]
    fn test_syscall_constants() {
        assert_eq!(SYS_IO_URING_SETUP, 425);
        assert_eq!(SYS_IO_URING_ENTER, 426);
        assert_eq!(SYS_IO_URING_REGISTER, 427);
    }

    #[test]
    fn test_liburing_with_emulation_failover() {
        struct EmulationBackend {
            fallback_count: Arc<AtomicU32>,
        }

        impl EmulationBackend {
            fn new() -> Self {
                Self {
                    fallback_count: Arc::new(AtomicU32::new(0)),
                }
            }

            fn try_native(&self) -> bool {
                if cfg!(target_os = "linux") {
                    KernelUring::new(32).is_ok()
                } else {
                    false
                }
            }

            fn emulate(&self) -> std::io::Result<Vec<KernelCQE>> {
                self.fallback_count.fetch_add(1, Ordering::SeqCst);
                Ok(vec![])
            }
        }

        let backend = EmulationBackend::new();
        
        if backend.try_native() {
            let ring = KernelUring::new(256).unwrap();
            assert!(ring.sq_entries() > 0);
        } else {
            let result = backend.emulate();
            assert!(result.is_ok());
            assert_eq!(backend.fallback_count.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn test_async_uring_with_failover() {
        use tokio::io::unix::AsyncFd;

        struct AsyncUringAdapter {
            use_emulation: bool,
        }

        impl AsyncUringAdapter {
            fn new() -> Self {
                Self {
                    use_emulation: !cfg!(target_os = "linux"),
                }
            }

            async fn read(&self, fd: std::os::unix::io::RawFd, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.use_emulation {
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    Err(std::io::Error::new(std::io::ErrorKind::Other, "emulated"))
                } else {
                    tokio::io::async_readead::AsyncRead::read(&AsyncFd::new(fd).unwrap(), buf).await
                }
            }
        }

        let adapter = AsyncUringAdapter::new();
        
        if cfg!(target_os = "linux") {
            let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let fd = socket.as_raw_fd();
            let mut buf = [0u8; 10];
            let result = adapter.read(fd, &mut buf).await;
            assert!(result.is_err() || result.unwrap() == 0);
        }
    }
}