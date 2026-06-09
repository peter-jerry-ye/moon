# MoonBuild Async Wasm Runtime

This context names the concepts used to describe `moonrun` support for running `moonbitlang/async` on the wasm backend.

## Language

**Semantic Stub Boundary**:
The operation-level contract exposed by `moonbitlang/async` native stubs, independent of native pointer layout or runtime object representation.
_Avoid_: Raw C ABI boundary

**Mapped Parity**:
A compatibility goal where wasm host behavior is tracked against native async stub operations without requiring a literal translation of the C files.
_Avoid_: Rewrite, line-by-line port

**Host Handle**:
An integer resource identity that the wasm guest can store and pass back while the host owns the underlying resource.
_Avoid_: Raw fd, raw HANDLE, externref

**Guest Owner Struct**:
A wasm-side value that keeps MoonBit-owned data reachable while the host has a pending operation referring to its guest-memory range.
_Avoid_: Pinned guest pointer

**Source Provenance Import**:
A `moonbit_v0` import declared together with the async C-stub source file and native symbol it tracks.
_Avoid_: Untraceable host helper

**Current Guest Memory**:
The `WebAssembly.Memory` object exposed as `moonbit_v0.memory` by the JS glue after instance creation or imported-memory discovery.
Host calls reacquire the current backing store for each import and never retain borrowed guest slices.
_Avoid_: Cached raw wasm pointer

## Boundary Decisions

- `moonrun` keeps V8 as the first adapter, but async host state remains outside V8 types.
- `moonbit_v0` imports strip the native `moonbitlang_async_` prefix and do not add an `async_` prefix.
- Native C stubs are the semantic reference. Rust code should stay structurally close to the source files, but it does not link against `moonbit.h` object layouts.
- Variable-length data crosses the boundary through guest offsets and explicit lengths. Async jobs store host-owned buffers plus guest offsets, then copy into freshly reacquired guest memory during a later host call.
- V8 memory growth can replace the observable memory backing store. The runtime must not lend guest pointers to OS APIs that need pinned buffers across `memory.grow`; use host-owned pinned buffers and copy to/from wasm memory instead.
- Windows APIs that require stable buffers should receive host-owned memory, not raw wasm memory. This includes overlapped IO and other APIs where the OS may retain a pointer until asynchronous completion.
