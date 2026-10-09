# Rust port implementation matrix — 1.1.0-alpha.1

**Verification status: not compiled** (Rust and FlatBuffers compiler unavailable in the build environment). All implementation claims below refer to code written, not validated runtime behavior.

| Design subsystem | Port status | Remaining verification |
|---|---|---|
| Cognitive Fabric | Written in Rust | Compile typed FlatBuffers IPC, frame tests |
| Micro-LLM adapter | Written | Verify against LM Studio on loopback |
| Jev+ | Heuristic router written | Measure quality and cost |
| Jev-R | Bounded role loop written | Verify calls and latency |
| Expert Registry | SQLite implementation written | Regression test dynamic skill addition |
| Memory | SQLite WAL + FTS5 written | Check storage migration, FTS test |
| Security | Deny-by-default capability checks | Independent sandbox/security audit |
| RAG candidates | Integrity + quality + approval gates written | Run held-out evaluation |
| Model training | Python script retained | Run training separately |
| LoRA hot swap / neural promotion | Not implemented | Requires separate research and reviews |
| Energy / GPU monitoring | Not implemented | Benchmark infrastructure |
| Novel learned Jev-R | Not implemented | Research program |

The original `v1.0.0` Python reference remains in Git history, and the Rust rewrite is isolated in branch `rust-core`. Persistent database storage uses a different default directory (`~/.local/share/cogfabric-rust`) to prevent accidental schema collisions with the legacy Python implementation.
