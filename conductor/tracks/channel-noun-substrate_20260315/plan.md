# Track: Channel noun as substrate for orbit ring architecture

## Objective

userspace::network::Channel is the typed async I/O noun flowing through
the orbit ring channelization architecture. Confirm it computes with Rust
and serves as the substrate literbike's reactor/modelmux builds on.

## Architecture position

```
userspace::kernel::endgame_bypass  — direct io_uring/eBPF syscall
userspace::network::Channel        — async read/write trait (the noun)
userspace::concurrency             — Kotlin-style coroutine scopes
         ↓
literbike::reactor                 — select-based event loop
literbike::modelmux::proxy         — OpenAI-compat HTTP proxy :8888
literbike::keymux::dsel            — provider routing + quota
         ↓
opencode (UI only, no model routing JS VM)
```

## Why no Bun in the routing path

Bun/JSC reserves ~485GB VSZ per process on Apple Silicon (Gigacage).
Running Bun as the model routing layer means JSC GC + Gigacage overhead
for every AI request. The Channel trait + reactor handles this in Rust
with zero GC overhead and no VSZ blowup.

Bun remains useful for: TypeScript tooling, file ops, test runner.
It does NOT need to be in the hot path for model API calls.

## Channel trait status

- [x] Channel: AsyncRead + AsyncWrite + Send + Sync + Unpin
- [x] TcpChannel, ChannelMetadata, ChannelProvider
- [x] endgame_bypass: direct sendmsg/recvmsg syscalls (Linux x86_64)
- [ ] macOS kqueue-backed selector (for Apple Silicon desktop)
- [ ] QUIC channel impl (literbike::quic as transport)
- [ ] rbcursive drives orbit I/O (pending cpp2 noun integration)

## cpp2 / orbit relationship

orbit_rings.cpp2 and orbit_scanner.cpp2 define the event schema
(OrbitEvent, ISAMCursor) in Herb Sutter's cpp2 syntax.
These are the structured nouns that flow through Channel.
The cpp2 compiler work (../cpp2/) is out of scope for this track
but the Channel trait must remain compatible with C-ABI export
so cpp2-generated code can interop via FFI.

## GitHub strategy

No Bun PRs needed. userspace is a private dependency of literbike.
Changes flow: userspace → literbike → opencode (via localhost:8888).
