# Wasm runtime evaluation

This isolated feasibility experiment measures a Wasm Component Model guest calling a host operation through tuilane's existing permission-aware `CommandDispatcher`. It is development evidence only: tuilane does not load external plugins in the product, and the WIT contract here is not a supported plugin API or wire format.

## Run the evaluation

Install the Rust WebAssembly target once, then run the experiment from the repository root:

```sh
rustup target add wasm32-unknown-unknown --toolchain stable
nix develop .#runtime-evaluation -c scripts/check-runtime-evaluation.sh
```

The script runs host tests, builds the guest, converts it to a component, checks its imports, and runs release-mode behavior checks and benchmarks. Build outputs are kept under the ignored `target/` directory.

## Method and results

The recorded run used stable Rust 1.96.0, Wasmtime 49.0.5, `wit-bindgen` 0.63.0, and `wasm-tools` 1.263.0 on Linux x86_64. Both guest and host were built in release mode. Results are local measurements, not cross-platform guarantees.

Startup medians are from five independent engine, component-load, and instance-creation cycles. Call latency is from 5,000 calls after 1,000 warm-up calls, using the same 18-byte echo payload and host dispatcher in both paths. Process RSS comes from `/proc/self/status`; it is a process-wide snapshot, not a per-allocation measurement.

| Measurement | Result |
|---|---:|
| Wasmtime engine creation, median | 10.41 µs |
| Component compile/load, median | 30.64 ms |
| Component instance creation, median | 67.60 µs |
| Cold end-to-end startup, median | 30.71 ms |
| Process RSS before runtime initialization | 3,336 KiB |
| Process RSS after engine and component load | 14,640 KiB |
| Direct dispatcher call, p50 / p95 | 140 ns / 141 ns |
| Wasm component call, p50 / p95 | 622 ns / 652 ns |
| Guest linear memory before / after 1 MiB allocation | 1,179,648 / 2,293,760 bytes |
| Guest memory after rejected 64 MiB allocation | 2,293,760 bytes |
| Fuel exhaustion / epoch interruption | 14.28 µs / 20.07 ms |

The measured component boundary added 482 ns at p50 and 511 ns at p95 for this small operation, about 4.4 times the direct dispatch latency. The absolute cost was below one microsecond in this run. Component loading dominated cold startup; the Wasmtime engine and component added 11,304 KiB to the observed process RSS before instance creation. The guest's configured linear-memory limit was 32 MiB.

The checks confirmed that the component imports only `tuilane:runtime-evaluation/host`, receives no caller identity from guest input, and has no WASI filesystem or network imports. An ungranted write was rejected by `CommandDispatcher` before its handler ran. Fuel limits and an epoch deadline interrupted an infinite guest loop. A trap, fuel exhaustion, and epoch interruption each left that instance unusable, so recovery requires discarding it and creating a fresh instance; that recovery path passed.

## Decision

**Go to the CH-10 SDK prototype using Wasmtime and WIT as the candidate runtime.** The measured startup and per-call overhead are acceptable for an initial host-mediated extension prototype, and the permission, memory, interruption, and replacement-instance checks behaved as required. CH-10 must still prove the manifest, install lifecycle, grant and revocation flow, compatibility errors, and independently authored sample extension.

This decision does not integrate a runtime into the application, stabilize an extension API, or prove general sandbox security. The experiment exposes one narrow host interface and has not tested hostile component corpora, concurrent plugin loads, cross-platform behavior, long-running workloads, or real feature commands. The host must keep imports explicit, enforce all permissions itself, bound guest resources, and discard instances after traps or interruptions.

A scripting-runtime comparison was not run. No scripting candidate is currently specified, and evaluating one would not improve this experiment's primary question: whether a portable component can call host-mediated capabilities with enforceable resource limits. Revisit that comparison if CH-10 authoring feedback identifies a concrete need.
