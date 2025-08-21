//! Tensor operations and MLIR coordination

pub mod core;
pub mod mlir;

pub use core::{Tensor, TensorShape, DType};
pub use mlir::{MLIRContext, MLIRTensor};