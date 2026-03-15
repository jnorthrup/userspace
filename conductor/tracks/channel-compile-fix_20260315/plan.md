# Track: Fix channel compile errors (unblocks literbike tests)

68 errors in userspace prevent `cargo test --lib` in literbike.
Core issues are in concurrency/channels/broadcast.rs and mod.rs.

## Errors
- `broadcast.rs:16,19,140` — unmatched angle brackets (extra `>`)
- `mod.rs:39` — Channel name collision (imported + redefined)
- `ChannelCapacity/RecvError/SendError` private item imports
- `T: Clone` + `Send` bounds not satisfied on generic channel impls
- `dyn Channel<T>` needs box indirection (E0782)

## Verification
`cargo check --lib 2>&1 | grep -c "^error"` → 0

## Status
- [ ] Fix angle brackets in broadcast.rs
- [ ] Resolve Channel name collision in mod.rs
- [ ] Fix private imports
- [ ] Satisfy trait bounds
- [ ] cargo check --lib clean
