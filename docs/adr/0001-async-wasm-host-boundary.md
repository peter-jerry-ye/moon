# Async Wasm Host Boundary

We will support `moonbitlang/async` on the wasm backend through `#cfg(target="wasm")` bindings to a `moonbit_v0` host module in `moonrun`, without compiler changes or JS backend changes. The host follows the semantic contract of async's native C stubs, but wasm resources are represented as host handles and guest-memory ranges rather than native MoonBit runtime objects or pinned pointers; this keeps V8 as the first adapter while allowing the V8-free host core to be reused by future wasm runtimes.
