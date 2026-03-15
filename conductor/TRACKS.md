# Conductor Tracks

Track 1: posix_sockets migration
- owner: master
- status: completed
- slice: migrate posix_sockets.rs from literbike (~130 lines)
- files: src/kernel/posix_sockets.rs (created)
- verify: cargo check --features syscall-net
- result: passed

Track 2: knox_proxy and tethering_bypass migration  
- owner: master
- status: completed
- slice: migrate knox_proxy and tethering_bypass from literbike
- files: src/kernel/knox_proxy.rs, src/kernel/tethering_bypass.rs (created)
- verify: cargo check --features kernel
- result: passed (17 tests)

Track 3: Unified kernel syscall interface
- owner: master
- status: completed
- slice: create single API surface for syscall adapters
- files: src/kernel/syscall.rs (created)
- verify: cargo check --features "kernel,syscall-net"
- result: passed (17 tests)

Track 4: Kernel feature tests
- owner: master
- status: completed
- slice: add integration and unit tests for kernel features
- verify: cargo test --lib --features "kernel,syscall-net"
- result: passed (17 tests)
