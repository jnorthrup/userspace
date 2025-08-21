//! Core tensor types and operations

use std::fmt;

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
    Bool,
}

impl DType {
    /// Size in bytes of this data type
    pub fn size(&self) -> usize {
        match self {
            DType::Bool | DType::U8 => 1,
            DType::F32 | DType::I32 | DType::U32 => 4,
            DType::F64 | DType::I64 | DType::U64 => 8,
        }
    }
}

/// Shape of a tensor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorShape {
    dims: Vec<usize>,
}

impl TensorShape {
    pub fn new(dims: Vec<usize>) -> Self {
        Self { dims }
    }
    
    pub fn rank(&self) -> usize {
        self.dims.len()
    }
    
    pub fn dims(&self) -> &[usize] {
        &self.dims
    }
    
    pub fn numel(&self) -> usize {
        self.dims.iter().product()
    }
    
    pub fn is_scalar(&self) -> bool {
        self.dims.is_empty()
    }
}

/// Basic tensor structure
pub struct Tensor {
    data: Vec<u8>,
    shape: TensorShape,
    dtype: DType,
    strides: Vec<usize>,
}

impl Tensor {
    /// Create a new tensor with uninitialized data
    pub fn uninit(shape: TensorShape, dtype: DType) -> Self {
        let numel = shape.numel();
        let size = numel * dtype.size();
        let data = vec![0u8; size];
        let strides = Self::compute_strides(&shape, dtype);
        
        Self {
            data,
            shape,
            dtype,
            strides,
        }
    }
    
    /// Create a tensor filled with zeros
    pub fn zeros(shape: TensorShape, dtype: DType) -> Self {
        Self::uninit(shape, dtype)
    }
    
    /// Create a tensor filled with ones
    pub fn ones(shape: TensorShape, dtype: DType) -> Self {
        let mut tensor = Self::uninit(shape, dtype);
        // Simplified - in real implementation would handle different dtypes
        if dtype == DType::F32 {
            let ones = unsafe {
                std::slice::from_raw_parts_mut(
                    tensor.data.as_mut_ptr() as *mut f32,
                    tensor.shape.numel(),
                )
            };
            ones.fill(1.0);
        }
        tensor
    }
    
    /// Compute strides for row-major layout
    fn compute_strides(shape: &TensorShape, dtype: DType) -> Vec<usize> {
        let mut strides = Vec::with_capacity(shape.rank());
        let mut stride = dtype.size();
        
        for &dim in shape.dims().iter().rev() {
            strides.push(stride);
            stride *= dim;
        }
        
        strides.reverse();
        strides
    }
    
    pub fn shape(&self) -> &TensorShape {
        &self.shape
    }
    
    pub fn dtype(&self) -> DType {
        self.dtype
    }
    
    pub fn strides(&self) -> &[usize] {
        &self.strides
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl fmt::Debug for Tensor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tensor")
            .field("shape", &self.shape)
            .field("dtype", &self.dtype)
            .field("size_bytes", &self.data.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tensor_shape() {
        let shape = TensorShape::new(vec![2, 3, 4]);
        assert_eq!(shape.rank(), 3);
        assert_eq!(shape.numel(), 24);
        assert!(!shape.is_scalar());
    }
    
    #[test]
    fn test_tensor_creation() {
        let shape = TensorShape::new(vec![2, 3]);
        let tensor = Tensor::zeros(shape, DType::F32);
        assert_eq!(tensor.shape().numel(), 6);
        assert_eq!(tensor.dtype(), DType::F32);
        assert_eq!(tensor.as_bytes().len(), 24); // 6 elements * 4 bytes
    }
}