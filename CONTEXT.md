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
