//! Memory-mapped tensor store for eBPF VM
//!
//! Provides mmap-backed storage for tensors with zero-copy typed access
//! and integration with the eBPF virtual machine.

use std::io;
use std::path::Path;
use std::sync::Arc;
use parking_lot::RwLock;
use memmap2::{MmapMut, MmapOptions};
use bytemuck::{Pod, cast_slice, cast_slice_mut};

/// Memory backend for eBPF VM and tensor storage
pub enum MemoryBackend {
    /// Heap-allocated memory (default)
    Heap(Vec<u8>),
    /// Memory-mapped file
    Mmap(Arc<RwLock<MmapMut>>),
}

impl MemoryBackend {
    /// Create a heap-backed memory region
    pub fn heap(size: usize) -> Self {
        MemoryBackend::Heap(vec![0u8; size])
    }
    
    /// Create a memory-mapped file backend
    pub fn mmap_file(path: &Path, size: usize) -> io::Result<Self> {
        use std::fs::OpenOptions;
        
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        
        file.set_len(size as u64)?;
        
        let mmap = unsafe {
            MmapOptions::new()
                .len(size)
                .map_mut(&file)?
        };
        
        Ok(MemoryBackend::Mmap(Arc::new(RwLock::new(mmap))))
    }
    
    /// Get the length of the memory region
    pub fn len(&self) -> usize {
        match self {
            MemoryBackend::Heap(v) => v.len(),
            MemoryBackend::Mmap(m) => m.read().len(),
        }
    }
    
    /// Get a read-only slice of the memory
    pub fn as_slice(&self) -> &[u8] {
        match self {
            MemoryBackend::Heap(v) => v.as_slice(),
            MemoryBackend::Mmap(_) => {
                // This is tricky - we can't return a reference from a RwLock guard
                // In practice, you'd use a different API or unsafe code
                panic!("Use with_slice for mmap backend")
            }
        }
    }
    
    /// Access the memory slice with a closure (works for both backends)
    pub fn with_slice<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        match self {
            MemoryBackend::Heap(v) => f(v.as_slice()),
            MemoryBackend::Mmap(m) => {
                let guard = m.read();
                f(&**guard)
            }
        }
    }
    
    /// Access the mutable memory slice with a closure
    pub fn with_mut_slice<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        match self {
            MemoryBackend::Heap(v) => f(v.as_mut_slice()),
            MemoryBackend::Mmap(m) => {
                let mut guard = m.write();
                f(&mut **guard)
            }
        }
    }
    
    /// Sync a range to disk (for mmap only)
    pub fn sync_range(&self, offset: usize, len: usize) -> io::Result<()> {
        match self {
            MemoryBackend::Heap(_) => Ok(()), // No-op for heap
            MemoryBackend::Mmap(m) => {
                let guard = m.read();
                guard.flush_range(offset, len)?;
                Ok(())
            }
        }
    }
    
    /// Advise the kernel about access patterns
    pub fn advise_willneed(&self, offset: usize, len: usize) -> io::Result<()> {
        match self {
            MemoryBackend::Heap(_) => Ok(()), // No-op for heap
            MemoryBackend::Mmap(m) => {
                let guard = m.read();
                guard.advise_range(memmap2::Advice::WillNeed, offset, len)?;
                Ok(())
            }
        }
    }
}

/// Data type for tensor elements
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F32,
    F64,
    I32,
    I64,
    U8,
    U32,
    U64,
}

impl DType {
    pub fn size(&self) -> usize {
        match self {
            DType::U8 => 1,
            DType::F32 | DType::I32 | DType::U32 => 4,
            DType::F64 | DType::I64 | DType::U64 => 8,
        }
    }
}

/// Handle to a tensor in memory
#[derive(Clone)]
pub struct TensorHandle {
    pub id: u64,
    pub dtype: DType,
    pub shape: Vec<usize>,
    pub strides: Vec<usize>,
    pub offset: usize,
    pub byte_len: usize,
    pub backend: Arc<MemoryBackend>,
}

impl TensorHandle {
    /// Create a new tensor handle
    pub fn new(
        id: u64,
        dtype: DType,
        shape: Vec<usize>,
        offset: usize,
        backend: Arc<MemoryBackend>,
    ) -> Self {
        let numel: usize = shape.iter().product();
        let byte_len = numel * dtype.size();
        let strides = Self::compute_strides(&shape, dtype);
        
        Self {
            id,
            dtype,
            shape,
            strides,
            offset,
            byte_len,
            backend,
        }
    }
    
    /// Compute strides for row-major layout
    fn compute_strides(shape: &[usize], dtype: DType) -> Vec<usize> {
        let mut strides = Vec::with_capacity(shape.len());
        let mut stride = dtype.size();
        
        for &dim in shape.iter().rev() {
            strides.push(stride);
            stride *= dim;
        }
        
        strides.reverse();
        strides
    }
    
    /// Get a typed slice of the tensor data (read-only)
    pub fn as_slice<T: Pod>(&self) -> Result<&[T], String> {
        if std::mem::size_of::<T>() != self.dtype.size() {
            return Err("Type size mismatch".to_string());
        }
        
        match &*self.backend {
            MemoryBackend::Heap(v) => {
                let bytes = &v[self.offset..self.offset + self.byte_len];
                Ok(cast_slice(bytes))
            }
            MemoryBackend::Mmap(_) => {
                Err("Use with_slice for mmap-backed tensors".to_string())
            }
        }
    }
    
    /// Access tensor data with a closure
    pub fn with_slice<T: Pod, F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&[T]) -> R,
    {
        if std::mem::size_of::<T>() != self.dtype.size() {
            return Err("Type size mismatch".to_string());
        }
        
        self.backend.with_slice(|bytes| {
            let tensor_bytes = &bytes[self.offset..self.offset + self.byte_len];
            let typed_slice = cast_slice(tensor_bytes);
            Ok(f(typed_slice))
        })
    }
}

/// Registry for managing tensors
pub struct TensorRegistry {
    tensors: RwLock<Vec<TensorHandle>>,
    next_id: AtomicU64,
}

use std::sync::atomic::{AtomicU64, Ordering};

impl TensorRegistry {
    pub fn new() -> Self {
        Self {
            tensors: RwLock::new(Vec::new()),
            next_id: AtomicU64::new(0),
        }
    }
    
    /// Register a new tensor
    pub fn register(&self, tensor: TensorHandle) -> u64 {
        let mut tensors = self.tensors.write();
        let id = tensor.id;
        tensors.push(tensor);
        id
    }
    
    /// Get a tensor by ID
    pub fn get(&self, id: u64) -> Option<TensorHandle> {
        let tensors = self.tensors.read();
        tensors.iter().find(|t| t.id == id).cloned()
    }
    
    /// Create a new tensor with automatic ID
    pub fn create_tensor(
        &self,
        dtype: DType,
        shape: Vec<usize>,
        offset: usize,
        backend: Arc<MemoryBackend>,
    ) -> TensorHandle {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let tensor = TensorHandle::new(id, dtype, shape, offset, backend);
        self.register(tensor.clone());
        tensor
    }
}

impl Default for TensorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Extended eBPF VM with tensor support
pub struct TensorVM {
    pub backend: Arc<MemoryBackend>,
    pub registry: Arc<TensorRegistry>,
    pub vm: super::ebpf::VM,
}

impl TensorVM {
    pub fn new(backend: MemoryBackend) -> Self {
        let backend = Arc::new(backend);
        let registry = Arc::new(TensorRegistry::new());
        let vm = super::ebpf::VM::new(0); // Will use backend instead
        
        Self {
            backend,
            registry,
            vm,
        }
    }
    
    /// Allocate a tensor in the VM's memory
    pub fn alloc_tensor(&self, dtype: DType, shape: Vec<usize>) -> Result<TensorHandle, String> {
        // In a real implementation, would track allocations
        let offset = 0; // Simplified - would use actual memory allocator
        Ok(self.registry.create_tensor(dtype, shape, offset, self.backend.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    
    #[test]
    fn test_memory_backend_heap() {
        let backend = MemoryBackend::heap(1024);
        assert_eq!(backend.len(), 1024);
        
        backend.with_slice(|slice| {
            assert_eq!(slice.len(), 1024);
        });
    }
    
    #[test]
    fn test_memory_backend_mmap() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.dat");
        
        let backend = MemoryBackend::mmap_file(&path, 4096).unwrap();
        assert_eq!(backend.len(), 4096);
        
        backend.with_slice(|slice| {
            assert_eq!(slice.len(), 4096);
        });
    }
    
    #[test]
    fn test_tensor_handle() {
        let backend = Arc::new(MemoryBackend::heap(1024));
        let tensor = TensorHandle::new(
            0,
            DType::F32,
            vec![2, 3],
            0,
            backend,
        );
        
        assert_eq!(tensor.shape, vec![2, 3]);
        assert_eq!(tensor.byte_len, 24); // 2*3*4 bytes
    }
    
    #[test]
    fn test_tensor_registry() {
        let registry = TensorRegistry::new();
        let backend = Arc::new(MemoryBackend::heap(1024));
        
        let tensor = registry.create_tensor(
            DType::F32,
            vec![10, 10],
            0,
            backend,
        );
        
        assert_eq!(tensor.id, 0);
        assert!(registry.get(0).is_some());
    }
}