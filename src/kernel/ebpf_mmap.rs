//! Memory-mapped tensor store for eBPF VM
//!
//! Provides mmap-backed storage for tensors with zero-copy typed access
//! and integration with the eBPF virtual machine.

use bytemuck::{cast_slice, cast_slice_mut, Pod};
use memmap2::{MmapMut, MmapOptions};
use parking_lot::RwLock;
use std::io;
use std::path::Path;
use std::sync::Arc;

/// Memory backend for eBPF VM and tensor storage
pub enum MemoryBackend {
    /// Heap-allocated memory (default) with interior mutability so it can be
    /// shared (`Arc`) and mutated through locks like the mmap variant.
    Heap(Arc<RwLock<Vec<u8>>>),
    /// Memory-mapped file
    Mmap(Arc<RwLock<MmapMut>>),
}

impl MemoryBackend {
    /// Create a heap-backed memory region
    pub fn heap(size: usize) -> Self {
        MemoryBackend::Heap(Arc::new(RwLock::new(vec![0u8; size])))
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

        let mmap = unsafe { MmapOptions::new().len(size).map_mut(&file)? };

        Ok(MemoryBackend::Mmap(Arc::new(RwLock::new(mmap))))
    }

    /// Get the length of the memory region
    pub fn len(&self) -> usize {
        match self {
            MemoryBackend::Heap(v) => v.read().len(),
            MemoryBackend::Mmap(m) => m.read().len(),
        }
    }

    /// Get a read-only view of the memory as an owned Vec.
    ///
    /// Callers that need to avoid copying should use `with_slice`.
    pub fn as_slice(&self) -> Vec<u8> {
        match self {
            MemoryBackend::Heap(v) => {
                let guard = v.read();
                guard.clone()
            }
            MemoryBackend::Mmap(m) => {
                let guard = m.read();
                (&**guard).to_vec()
            }
        }
    }

    /// Access the memory slice with a closure (works for both backends)
    pub fn with_slice<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        match self {
            MemoryBackend::Heap(v) => {
                let guard = v.read();
                f(&**guard)
            }
            MemoryBackend::Mmap(m) => {
                let guard = m.read();
                f(&**guard)
            }
        }
    }

    /// Access the mutable memory slice with a closure
    pub fn with_mut_slice<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        match self {
            MemoryBackend::Heap(v) => {
                let mut guard = v.write();
                f(&mut **guard)
            }
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
    pub fn as_slice<T: Pod>(&self) -> Result<Vec<T>, String> {
        if std::mem::size_of::<T>() != self.dtype.size() {
            return Err("Type size mismatch".to_string());
        }

        match &*self.backend {
            MemoryBackend::Heap(v) => {
                let guard = v.read();
                let bytes = &guard[self.offset..self.offset + self.byte_len];
                let typed = cast_slice::<u8, T>(bytes);
                Ok(typed.to_vec())
            }
            MemoryBackend::Mmap(_) => Err("Use with_slice for mmap-backed tensors".to_string()),
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

    /// Mutable access to tensor data with a closure. Works for both heap and mmap.
    pub fn with_slice_mut<T: Pod, F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut [T]) -> R,
    {
        if std::mem::size_of::<T>() != self.dtype.size() {
            return Err("Type size mismatch".to_string());
        }

        // Use backend's mutable access
        let res = self.backend.with_mut_slice(|bytes| {
            let tensor_bytes = &mut bytes[self.offset..self.offset + self.byte_len];
            // Safety: Pod ensures alignment/representation; cast_slice_mut returns &mut [T]
            let typed = cast_slice_mut(tensor_bytes);
            f(typed)
        });

        Ok(res)
    }

    /// In-place add scalar for f32 tensors (simple implementation)
    #[inline]
    pub fn add_scalar_f32_inplace(&self, scalar: f32) -> Result<(), String> {
        if self.dtype != DType::F32 {
            return Err("dtype mismatch: expected F32".to_string());
        }

        self.with_slice_mut::<f32, _, _>(|data| {
            for v in data.iter_mut() {
                *v += scalar;
            }
        })?;

        Ok(())
    }

    /// Unrolled "wide" in-place add for f32 tensors (processes 4 elements per iteration).
    /// This is a simple, portable registerization hint via loop unrolling.
    #[inline]
    pub fn add_scalar_f32_inplace_wide(&self, scalar: f32) -> Result<(), String> {
        if self.dtype != DType::F32 {
            return Err("dtype mismatch: expected F32".to_string());
        }

        self.with_slice_mut::<f32, _, _>(|data| {
            let n = data.len();
            let mut i = 0usize;
            while i + 4 <= n {
                // unrolled add of 4 values
                data[i] += scalar;
                data[i + 1] += scalar;
                data[i + 2] += scalar;
                data[i + 3] += scalar;
                i += 4;
            }
            // tail
            while i < n {
                data[i] += scalar;
                i += 1;
            }
        })?;

        Ok(())
    }
}

/// Registry for managing tensors
pub struct TensorRegistry {
    tensors: RwLock<Vec<TensorHandle>>,
    next_id: AtomicU64,
}

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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
    /// Next free offset (simple bump allocator)
    pub next_offset: AtomicUsize,
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
            next_offset: AtomicUsize::new(0),
        }
    }

    /// Allocate a tensor in the VM's memory
    pub fn alloc_tensor(&self, dtype: DType, shape: Vec<usize>) -> Result<TensorHandle, String> {
        // Simple bump allocator: compute required bytes and allocate aligned to dtype size
        let numel: usize = shape.iter().product();
        let byte_len = numel
            .checked_mul(dtype.size())
            .ok_or_else(|| "size overflow".to_string())?;
        let align = dtype.size();
        // round up to alignment
        let alloc = (byte_len + align - 1) / align * align;

        let offset = self.next_offset.fetch_add(alloc, Ordering::SeqCst);

        // Ensure we don't allocate beyond backend length
        if offset
            .checked_add(byte_len)
            .map_or(true, |end| end > self.backend.len())
        {
            return Err("out of memory".to_string());
        }

        Ok(self
            .registry
            .create_tensor(dtype, shape, offset, self.backend.clone()))
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

        // as_slice returns a borrowed slice for heap
        let view = backend.as_slice();
        assert_eq!(view.len(), 1024);
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

        // as_slice will return an owned Vec wrapped in Cow for mmap
        let view = backend.as_slice();
        assert_eq!(view.len(), 4096);
    }

    #[test]
    fn test_tensor_handle() {
        let backend = Arc::new(MemoryBackend::heap(1024));
        let tensor = TensorHandle::new(0, DType::F32, vec![2, 3], 0, backend);

        assert_eq!(tensor.shape, vec![2, 3]);
        assert_eq!(tensor.byte_len, 24); // 2*3*4 bytes
    }

    #[test]
    fn test_tensor_registry() {
        let registry = TensorRegistry::new();
        let backend = Arc::new(MemoryBackend::heap(1024));

        let tensor = registry.create_tensor(DType::F32, vec![10, 10], 0, backend);

        assert_eq!(tensor.id, 0);
        assert!(registry.get(0).is_some());
    }

    #[test]
    fn test_tensor_vm_alloc() {
        let backend = MemoryBackend::heap(1024);
        let vm = TensorVM::new(backend);

        let t1 = vm.alloc_tensor(DType::F32, vec![4, 4]).expect("alloc1"); // 16 * 4 = 64 bytes
        let t2 = vm.alloc_tensor(DType::F32, vec![8, 8]).expect("alloc2"); // 64 * 4 = 256 bytes

        // Ensure offsets are distinct and non-overlapping
        assert_ne!(t1.offset, t2.offset);
        let end1 = t1.offset + t1.byte_len;
        let end2 = t2.offset + t2.byte_len;

        assert!(
            end1 <= t2.offset || end2 <= t1.offset,
            "allocations overlap"
        );
        // Ensure within backend
        assert!(end2 <= vm.backend.len());
    }

    #[test]
    fn test_with_slice_mut_heap() {
        let backend = Arc::new(MemoryBackend::heap(256));
        let tensor = TensorHandle::new(0, DType::F32, vec![4, 4], 0, backend.clone());

        // write to tensor
        tensor
            .with_slice_mut::<f32, _, _>(|data| {
                for (i, v) in data.iter_mut().enumerate() {
                    *v = i as f32;
                }
            })
            .expect("mut write");

        // read back
        tensor
            .with_slice::<f32, _>(|data| {
                assert_eq!(data[0], 0.0);
                assert_eq!(data[5], 5.0);
            })
            .expect("read");
    }

    #[test]
    fn test_with_slice_mut_mmap() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_mut.dat");
        let backend = MemoryBackend::mmap_file(&path, 4096).unwrap();
        let backend = Arc::new(backend);

        let tensor = TensorHandle::new(1, DType::U8, vec![16], 0, backend.clone());

        // mutate via with_slice_mut
        tensor
            .with_slice_mut::<u8, _, _>(|data| {
                for (i, v) in data.iter_mut().enumerate() {
                    *v = (i % 256) as u8;
                }
            })
            .expect("mmap mut");

        // ensure data persisted in mmap view
        backend.with_slice(|bytes| {
            assert_eq!(bytes[0], 0u8);
            assert_eq!(bytes[15], 15u8);
        });

        // sync range (should be no-op on heap, and flush for mmap)
        backend.sync_range(0, 16).expect("sync");
    }

    #[test]
    fn test_add_scalar_f32_heap_and_wide() {
        let backend = Arc::new(MemoryBackend::heap(1024));
        let tensor = TensorHandle::new(0, DType::F32, vec![8, 8], 0, backend.clone());

        // initialize
        tensor
            .with_slice_mut::<f32, _, _>(|data| {
                for (i, v) in data.iter_mut().enumerate() {
                    *v = i as f32;
                }
            })
            .unwrap();

        tensor.add_scalar_f32_inplace(1.0).unwrap();

        tensor
            .with_slice::<f32, _>(|data| {
                assert_eq!(data[0], 1.0);
                assert_eq!(data[10], 11.0);
            })
            .unwrap();

        tensor.add_scalar_f32_inplace_wide(2.0).unwrap();

        tensor
            .with_slice::<f32, _>(|data| {
                assert_eq!(data[0], 3.0);
                assert_eq!(data[10], 13.0);
            })
            .unwrap();
    }

    #[test]
    fn test_add_scalar_f32_mmap() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_add.dat");
        let backend = MemoryBackend::mmap_file(&path, 4096).unwrap();
        let backend = Arc::new(backend);

        let tensor = TensorHandle::new(1, DType::F32, vec![4, 4], 0, backend.clone());

        tensor
            .with_slice_mut::<f32, _, _>(|data| {
                for (i, v) in data.iter_mut().enumerate() {
                    *v = i as f32;
                }
            })
            .unwrap();

        tensor.add_scalar_f32_inplace(0.5).unwrap();

        tensor
            .with_slice::<f32, _>(|data| {
                assert!((data[0] - 0.5).abs() < 1e-6);
                assert!((data[5] - 5.5).abs() < 1e-6);
            })
            .unwrap();
    }
}
