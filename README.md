# phantomlinear

A dependency-free, safe-Rust implementation of explicit-consumption guidance, inspired by Aidan Cully's “[Less Painful Linear Types](https://aidancully.blogspot.com/2021/12/less-painful-linear-types.html)” (2021-12-11).

## Run

```sh
cargo fmt --all
cargo test --all-targets
cargo test --doc
cargo clippy --all-targets -- -D warnings
cargo run --example transaction
```

## Surface

- PhantomLinear: armed runtime drop check with a diagnostic name; complete(self)
  discharges it. Neither Copy nor Clone. Panics on normal accidental drop and
  suppresses its own panic when the thread is already unwinding.
- Linear<T>: convenience owner; get/get_mut, into_inner, consume, map and
  with_cleanup. Extraction transfers responsibility, not proof of finalization.
- Consume: user-defined consuming cleanup protocol.
- ReScopeDrop<T>: scope adapter invoking Consume on drop; extraction disables it.
- Cleanup<T,F>: scope adapter using an FnOnce(T) closure; supports run, extraction,
  borrowing, mutation, early returns, and unwinding.
- cleanup!: hygienic construction macro, each expression evaluated once.

## Why no ScopeDrop trait?

A library cannot teach Rust's drop checker to prohibit normal scope exit for
selected types. A user-defined trait, even an auto trait on nightly, would not
change that rule. The proposal's implicit ScopeDrop bounds, ?ScopeDrop syntax,
compiler diagnostics and automatic structural propagation require language and
compiler support. A conventional unsafe marker trait would misleadingly imply
an invariant this crate cannot enforce, so none is provided.

## Guarantees and non-guarantees

Rust ownership prevents consuming the same non-Copy value twice. The marker
checks omission only if its destructor actually runs during non-panicking
execution. Enclosing structs and containers eventually drop marker fields,
so omission is usually caught at runtime; must_use linting is not structurally
inherited. Both the marker and Linear carry must_use annotations, but bindings,
explicit drop, and let _ expressions can bypass that lint.

This marker has state and is not zero-sized. It changes struct layout and
prevents Copy. Do not use it in repr(C) or FFI layouts without deliberate ABI
review. Send and Sync derive structurally; no unsafe implementations are used.

Leaking with mem::forget, ManuallyDrop, Box::leak, or reference cycles bypasses
checks. Abort, process exit and non-termination also prevent cleanup. This is
not a memory-safety mechanism, a security boundary, or an exactly-once guarantee
for effects. A consumer can do nothing or leak its argument. An extracted value
is no longer guarded. Mutable access allows replacement of the original value.
Use a private domain-specific type with narrowly exposed terminal operations
when protocol fidelity matters.

## Panic and cleanup policy

The marker deliberately permits panic unwinding, matching the supplied proposal.
Cleanup adapters still execute their callbacks while unwinding. A callback that
panics during unwinding can abort the process; the crate does not catch or hide
such failures. Fallback callbacks must be simple and non-panicking. Run fallible
or asynchronous finalization explicitly rather than from Drop. Dropping an
async task's future runs synchronous destructors; this crate does not make
cancellation await cleanup or require polling a future to completion.

Option::take empties a guard before user cleanup runs, so extraction cannot
cause double cleanup and callback panics cannot cause a second invocation.
Discarded closure captures are dropped normally. No unsafe code, allocation,
trait objects or dependencies are needed by the library implementation.
Option storage and marker metadata are not promised to have zero overhead.

## Testing scope

Tests cover marker completion and accidental drops, nested/container ownership,
unwinding, borrowing, extraction, payload destruction, map and consumer panics,
FnOnce closures, captured-value destruction, early return via ?, callback panic,
Consume adapters, deliberate leaks, macro evaluation and Send/Sync for ordinary
payloads. The compile-fail doctest covers use after ownership transfer. Double
panic/abort is documented rather than triggered in-process by the test suite.

## Design choices

The article's pseudocode used ManuallyDrop and FnMut. Here, private Option
storage avoids unsafe extraction and FnOnce admits consuming captures. No
Clone, Copy, blanket Consume, automatic consuming Drop for Linear, or inert
ScopeDrop emulation is supplied. The generic Linear wrapper exposes protocol
transfer; the transaction example demonstrates a stricter private marker API.
