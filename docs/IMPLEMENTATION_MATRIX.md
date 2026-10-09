# Release 1.0 implementation matrix

**Definition of 1.0:** tested, local-first research orchestration runtime.
It does **not** mean that the full scientific architecture is achieved.

| Design subsystem | Runtime status | Evidence / limitation |
|---|---|---|
| Cognitive Fabric | **Implemented** | Versioned length-prefixed MessagePack, Unix socket, request validation |
| FlatBuffers | **Design only** | `.fbs` proposal; runtime is MessagePack, the approved equivalent |
| Micro-LLM interface | **Integrated adapter** | Local OpenAI-compatible endpoint; model not bundled |
| Jev+ decision engine | **Heuristic prototype** | Skill scoring and bounded role scheduling; no learned Jev decision head |
| Jev-R Collective | **Logical roles** | Planner/Solver/Explorer/Critic calls; no dedicated neural recursive model |
| Expert Registry | **Implemented for text skills** | Dynamic tags, RAG documents, candidate versions |
| Binary skill networking | **Partial** | Unix socket request/reply, not a distributed event mesh |
| Cognitive memory | **Implemented** | SQLite WAL, FTS5, task history and event traces |
| Safety kernel | **Initial reference design** | Deny-by-default capability registry, no arbitrary command execution |
| RAG evolution | **Manual-gated** | Candidate-specific retrieval quality, checksum, operator approval |
| LoRA training | **Optional offline script** | Requires external packages and model; no verified training run |
| LoRA hot swap | **Not implemented** | No runtime adapter manager or neural model-evaluator |
| Neural-weight promotion | **Blocked** | Intentionally disabled in 1.0 |
| Model self-rebuild | **Not implemented** | Separate R&D project |
| Auto dataset discovery | **Not implemented** | Security/licensing assessment needed |
| Performance benchmark | **Initial harness** | Per-task wall-clock, calls, accuracy; no joules/FLOPs |
| Distributed GPU scheduler | **Not implemented** | Current scheduler is bounded task-level control |
| Proven AGI/frontier performance | **Not claimed** | No independently validated comparison |

The terminology in the specification denotes *architectural roles*, not
necessarily distinct trained neural networks. Only measured improvements
should be represented as demonstrated scientific results.
