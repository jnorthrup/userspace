# Track: Userspace NIO SPI Facades (macOS Knox → literbike syscall introspection)

Expose kernel IO operations (io_uring, socket syscalls, mmap) as userspace Service Provider Interfaces. literbike routes through these facades while observing bun's jemalloc VSZ stupidity unfold.

## Scope
- `src/kernel/io_uring.rs` → public SPI facade
- `src/kernel/posix_sockets.rs` → public SPI facade
- `src/syscall_net.rs` → wire as MCP observable tools
- New: `src/nio/mod.rs` — unified NIO abstraction layer

## Key facades
- `socket_create()` — syscall wrapper, log VSZ at call time
- `socket_read/write()` — observe buffer sizes, allocation patterns
- `mmap_region()` — track virtual address reservations
- `io_uring_submit()` — profile async IO vs bun's sync stalls

## Multicast observer model

SPI is a multicast tap — every LLM call through modelmux fires NIO SPI events
to all registered observers simultaneously without overhead in the hot path:

```
modelmux :8888
    ↓ NIO SPI socket_write()
    ├── upstream provider (the actual call)
    └── observer multicast:
        ├── keymux quota tracker (token counting via event push, not poll)
        ├── latency histogram (per provider)
        ├── provider health state machine
        └── MCP tools (real-time Claude visibility into all LLM traffic)
```

No polling. No instrumentation in provider code. The SPI fires on every
socket_write/read to any upstream — observers see every byte across all providers.

## Integration
- literbike calls these facades transparently
- keymux quota tracking moves from poll → SPI event push
- opencode MCP tools expose them for real-time observation
- bun process runs alongside, both logged to same metric stream

## Verification
- `cargo check --lib` clean
- MCP tools callable from literbike HTTP ingress
- Profile output shows VSZ/syscall correlation

## Status
- [ ] Stub SPI surface for all io_uring operations
- [ ] Stub SPI surface for socket operations
- [ ] Wire as literbike MCP tools
- [ ] Run while-true loop, capture VSZ/syscall diffs
