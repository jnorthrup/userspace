# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**userspace** - A comprehensive userspace kernel emulation library providing:
- Structured concurrency patterns (Kotlin coroutines style)
- io_uring userspace implementation
- eBPF JIT compilation and execution
- Network protocol abstractions and adapters
- Tensor operations with MLIR coordination

The project consolidates various userspace kernel emulation components into a single coherent library.

## Development Commands

```bash
# Build the project (default features: concurrency)
cargo build

# Build with all features
cargo build --all-features

# Build with specific features
cargo build --features kernel
cargo build --features network
cargo build --features tensor

# Run tests
cargo test

# Check for compilation errors
cargo check

# Run clippy for linting
cargo clippy
```

## Architecture

### Module Organization

```
src/
├── concurrency/        # Structured concurrency (Kotlin coroutines style)
│   ├── job.rs         # Cancellable execution units
│   ├── scope.rs       # Coroutine scopes
│   ├── dispatcher.rs  # Thread execution contexts
│   ├── launch.rs      # Coroutine creation
│   ├── deferred.rs    # Future-like results
│   └── cancel.rs      # Cancellation tokens
├── kernel/            # Userspace kernel emulation
│   ├── io_uring.rs   # io_uring implementation
│   ├── nio.rs        # Non-blocking I/O
│   ├── ebpf.rs       # eBPF VM and JIT
│   └── ebpf_mmap.rs  # Memory-mapped tensor store
├── network/           # Network protocol abstractions
│   ├── adapters.rs   # HTTP, QUIC, SSH adapters
│   ├── protocols.rs  # Protocol detection
│   └── channels.rs   # Channel providers
└── tensor/           # Tensor operations
    ├── core.rs       # Core tensor types
    └── mlir.rs       # MLIR coordination
```

### Structured Concurrency

This module provides Kotlin coroutines-equivalent APIs in Rust:

**Core Components:**
- **Job** (`job.rs`): Cancellable execution units with hierarchical cancellation
  - `JobImpl`: Standard job with child cancellation propagation
  - `SupervisorJobImpl`: Jobs that don't cancel children when cancelled
- **CoroutineScope** (`scope.rs`): Defines execution context and lifecycle
  - `StandardCoroutineScope`: Regular scoped execution
  - `GlobalScope`: Application-lifetime singleton scope
- **Dispatchers** (`dispatcher.rs`): Thread execution contexts
  - `DefaultDispatcher`: Multi-threaded execution
  - `MainDispatcher`: Main thread confined execution
  - `IoDispatcher`: IO-optimized execution
  - `CpuDispatcher`: CPU-intensive workload execution
  - `LimitedDispatcher`: Parallelism-limited execution
- **Launch Functions** (`launch.rs`): Coroutine creation
  - `launch()`: Fire-and-forget coroutines returning Job
  - `async_coroutine()`: Result-returning coroutines (Deferred)
  - `run_blocking()`: Bridge to synchronous code
- **Deferred** (`deferred.rs`): Future-like objects with cancellation support
- **Cancellation** (`cancel.rs`): Cancellation token system

**Key Patterns:**
- Structured concurrency ensures child coroutines are cancelled when parents cancel
- All async operations are cancellation-aware
- Dispatchers provide different execution contexts (main thread, IO pool, CPU pool)
- Jobs form hierarchies for coordinated lifecycle management

### Module Integration Points

The main structured concurrency module serves as the foundation. Other modules (nio, uring, ebpf) are intended to integrate with this concurrency model to provide:
- Non-blocking I/O operations that respect coroutine cancellation
- High-performance async I/O via io_uring
- JIT-compiled eBPF programs for kernel-space execution

### Dependencies

- **tokio**: Async runtime and utilities
- **futures**: Stream and async abstractions  
- **tracing**: Structured logging
- **pin-project-lite**: Safe pinning utilities

## Testing

Tests are embedded in each source file using `#[tokio::test]` attributes. The test suite covers:
- Basic coroutine launching and completion
- Hierarchical job cancellation 
- Dispatcher behavior
- Deferred result handling
- Cancellation token propagation

All tests use the Tokio async test framework and should be run with `cargo test`.

## Migration TODOs

This project consolidates features from sibling projects (`literbike`, `betanet`) into `userspace`.
Below are tracked migration and consolidation tasks. Checkboxes reflect outstanding work.

- [x] Create `src/kernel/ebpf_mmap.rs` — mmap-backed tensor store and integration with `ebpf` VM
- [x] Migrate `syscall_net.rs` from `literbike` (~887 lines)
- [x] Migrate `posix_sockets.rs` from `literbike` (~130 lines)
- [x] Extract `endgame_kernel_bypass.rs` from `betanet` (as `endgame_bypass.rs`)
- [x] Consolidate `io_uring` implementations (betanet vs userspace) — single impl in `io_uring.rs`
- [x] Migrate `knox_proxy` and `tethering_bypass` from `literbike`
- [x] Create unified kernel syscall interface (single API surface for syscall adapters)
- [x] Test consolidated kernel features (integration + unit tests)

Notes:

- Prioritize migrating the `io_uring` implementations and creating a unified syscall interface to simplify downstream integrations.
- Use feature flags to gate platform-specific implementations during migration (e.g., `kernel`, `kernel-ebpf`, `kernel-tensor`).

### Network & syscall migrations

Track the network and syscall migration tasks pulled from `literbike` and `betanet`:

- [x] Migrate `syscall_net.rs` from `literbike` (887 lines)
- [x] Migrate `syscall_net.rs` from `literbike` (extracted to `src/kernel/syscall_net.rs`)
- [x] Migrate `posix_sockets.rs` from `literbike` (130 lines)
- [x] Extract `endgame_kernel_bypass.rs` from `betanet` (as `endgame_bypass.rs`)
- [x] Consolidate `io_uring` implementations (single impl in `src/kernel/io_uring.rs`)
- [x] Migrate `knox_proxy` and `tethering_bypass` from `literbike`
- [x] Create unified kernel syscall interface
- [x] Test consolidated kernel features
