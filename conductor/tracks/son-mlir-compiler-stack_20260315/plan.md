# Track: SON Compiler → MLIR (Critical Path to Flight)

Trajectory: SON spec → parser → MLIR lowering → literbike/modelmux ingress → MCP syscall introspection → Claude feedback loop.

## Phase 1: SON Language Definition (Gate on everything)
- [ ] SON syntax spec (grammar, operators, control flow)
- [ ] SON → Rust AST mapping
- [ ] Parser in userspace/src/compiler/son.rs

## Phase 2: MLIR Lowering
- [ ] SON AST → MLIR IR operations
- [ ] Type system alignment (SON types ↔ MLIR types)
- [ ] Lowering passes (const folding, inlining, etc.)
- [ ] Verify MLIR output compiles with mlir-opt

## Phase 3: Literbike HTTP Ingress + modelmux Routing
- [ ] literbike listening on :8888
- [ ] modelmux requestfactory routes `/compile` POST → compiler
- [ ] Request: `{src: "SON code", optimize: bool}`
- [ ] Response: `{mlir: "...", compile_time_ms: N, peak_rss: N}`

## Phase 4: Userspace MCP Introspection (Syscall Observable)
- [ ] MCP tools for: `list_syscalls`, `get_memory_timeline`, `profile_compilation`
- [ ] Hook syscall_net.rs into compilation invocations
- [ ] Stream VSZ/RSS deltas back to Claude during compile

## Phase 5: Claude Feedback Loop
- [ ] Claude submits SON → sees MLIR output + syscall profile
- [ ] Claude optimizes SON syntax or compiler passes
- [ ] Repeat

## Finish Line
SON code submitted via HTTP → compiled to MLIR → syscalls observed → Claude refines = airplane airborne.

## Status
- Phase 1: [ ] SON spec drafted
- Phase 2: [ ] MLIR lowering prototyped
- Phase 3: [ ] literbike/modelmux wired
- Phase 4: [ ] MCP tools active
- Phase 5: [ ] Loop converging
