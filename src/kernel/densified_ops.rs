//! Densified operations - Zero-allocation buffer transformations
//!
//! Applies densification principles to eliminate all no-ops in tight loops.
//! Every CPU cycle performs useful work - no speculative stalls or defensive checks.

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Densified memcpy using SIMD operations when possible
/// Eliminates all branching in the critical path for maximum throughput
#[cfg(target_arch = "x86_64")]
#[inline(always)]
/// # Safety
///
/// Caller must ensure that `src` and `dst` are valid for reads/writes of
/// `len` bytes respectively. The regions must not overlap.
pub unsafe fn densified_copy(src: *const u8, dst: *mut u8, len: usize) {
    // Prefetch both source and destination to avoid cache misses
    _mm_prefetch(src as *const i8, _MM_HINT_T0);
    _mm_prefetch(dst as *const i8, _MM_HINT_T0);
    
    let mut remaining = len;
    let mut src_ptr = src;
    let mut dst_ptr = dst;
    
    // Process 32-byte chunks using AVX2 (256-bit registers)
    #[cfg(target_feature = "avx2")]
    while remaining >= 32 {
        let chunk = _mm256_loadu_si256(src_ptr as *const __m256i);
        _mm256_storeu_si256(dst_ptr as *mut __m256i, chunk);
        
        src_ptr = src_ptr.add(32);
        dst_ptr = dst_ptr.add(32);
        remaining -= 32;
    }
    
    // Process 16-byte chunks using SSE2
    while remaining >= 16 {
        let chunk = _mm_loadu_si128(src_ptr as *const __m128i);
        _mm_storeu_si128(dst_ptr as *mut __m128i, chunk);
        
        src_ptr = src_ptr.add(16);
        dst_ptr = dst_ptr.add(16);
        remaining -= 16;
    }
    
    // Process 8-byte chunks using scalar u64 operations
    while remaining >= 8 {
        let chunk = (src_ptr as *const u64).read_unaligned();
        (dst_ptr as *mut u64).write_unaligned(chunk);
        
        src_ptr = src_ptr.add(8);
        dst_ptr = dst_ptr.add(8);
        remaining -= 8;
    }
    
    // Handle remaining bytes (0-7)
    for i in 0..remaining {
        *dst_ptr.add(i) = *src_ptr.add(i);
    }
}

/// Fallback implementation for non-x86_64 architectures
#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
pub unsafe fn densified_copy(src: *const u8, dst: *mut u8, len: usize) {
    std::ptr::copy_nonoverlapping(src, dst, len);
}

/// Densified comparison - returns 0 if equal, non-zero if different
#[inline(always)]
/// # Safety
///
/// Caller must ensure that `a` and `b` are valid for reads of `len` bytes.
pub unsafe fn densified_compare(a: *const u8, b: *const u8, len: usize) -> i32 {
    let mut i = 0usize;
    
    // Compare u64 chunks for speed
    while i + 8 <= len {
        let va = (a.add(i) as *const u64).read_unaligned();
        let vb = (b.add(i) as *const u64).read_unaligned();
        if va != vb {
            // Found difference, narrow down to exact byte
            for j in 0..8 {
                let ba = *a.add(i + j);
                let bb = *b.add(i + j);
                if ba != bb {
                    return ba as i32 - bb as i32;
                }
            }
        }
        i += 8;
    }
    
    // Compare remaining bytes
    while i < len {
        let ba = *a.add(i);
        let bb = *b.add(i);
        if ba != bb {
            return ba as i32 - bb as i32;
        }
        i += 1;
    }
    
    0
}

/// Densified XOR operation
#[inline(always)]
/// # Safety
///
/// Caller must ensure all pointers are valid for `len` bytes.
pub unsafe fn densified_xor(a: *const u8, b: *const u8, dst: *mut u8, len: usize) {
    let mut i = 0usize;
    
    // Process u64 chunks
    while i + 8 <= len {
        let va = (a.add(i) as *const u64).read_unaligned();
        let vb = (b.add(i) as *const u64).read_unaligned();
        (dst.add(i) as *mut u64).write_unaligned(va ^ vb);
        i += 8;
    }
    
    // Process remaining bytes
    while i < len {
        *dst.add(i) = *a.add(i) ^ *b.add(i);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_densified_copy() {
        let src = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut dst = vec![0u8; 8];
        
        unsafe {
            densified_copy(src.as_ptr(), dst.as_mut_ptr(), 8);
        }
        
        assert_eq!(src, dst);
    }
    
    #[test]
    fn test_densified_compare() {
        let a = vec![1u8, 2, 3, 4];
        let b = vec![1u8, 2, 3, 4];
        let c = vec![1u8, 2, 4, 4];
        
        unsafe {
            assert_eq!(densified_compare(a.as_ptr(), b.as_ptr(), 4), 0);
            assert_ne!(densified_compare(a.as_ptr(), c.as_ptr(), 4), 0);
        }
    }
    
    #[test]
    fn test_densified_xor() {
        let a = vec![0xFF, 0x00, 0xFF, 0x00];
        let b = vec![0x00, 0xFF, 0xFF, 0x00];
        let mut dst = vec![0u8; 4];
        
        unsafe {
            densified_xor(a.as_ptr(), b.as_ptr(), dst.as_mut_ptr(), 4);
        }
        
        assert_eq!(dst, vec![0xFF, 0xFF, 0x00, 0x00]);
    }
}