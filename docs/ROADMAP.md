# Cognitive Fabric roadmap

This is an implementation roadmap for the framework. The canonical project vision
and its original V0.1–V1.0 roadmap remain in [the French architecture](ARCHITECTURE.md).
This document adds delivery order and explicit post-core work; it does not claim
that planned items are implemented.

## Core completion: client-driven Jev-R orchestration

**Status: Partial / not yet complete.** An opt-in [per-request role graph](CLIENT_COLLECTIVE.md) now runs client conversations with bounded planning, exploration and solving. Durable cross-request collective continuation is still missing. Extend the client-facing agent path so
client conversation turns can be handled by bounded Jev+ routing and Jev-R
planning, solving, exploration, critique, verification and revision. Keep this
path compatible with clients that own their MCP servers, skills, web connections,
tool permissions and tool execution.

The runtime must accept the client's current tool catalog and conversation history,
propose tool calls, then continue from the correlated results or errors returned by
the client. It must retain the same configured model while applying the requested
reasoning effort. The client remains responsible for executing and authorizing its
tools; model output and tool results remain untrusted data. Jev+ and Jev-R roles,
per-call and whole-task budgets, cancellation, audit events and verification
evidence must remain explicit. A sequence of ordinary single model calls is not by
itself evidence that full Jev-R orchestration has been delivered.

**Completion evidence:** deterministic lifecycle tests for planning through
verification, tool-call/result correlation across resumed client turns, budget and
cancellation denials, audit-failure behavior, and end-to-end tests through the
client-facing server path. These tests establish protocol and orchestration
behavior, not general reasoning quality.

## Client protocol adapters

After the shared orchestration contract is stable, add protocol adapters where
there is a demonstrated client need. OpenAI-compatible text/function Chat
Completions remains one adapter. Candidate future adapters include Anthropic
Messages and other agent-client protocols. Each adapter should translate into the
same internal typed conversation and tool-call representation, preserve tool
results and errors, reject unsupported fields explicitly, and keep permission and
resource policy in Rust services. Do not imply compatibility with a protocol until
its request/response semantics and client workflows have tests.

## Optional multimodal evidence — final roadmap phase

**Priority: last, after the core roadmap and client orchestration are complete.**
Multimodal input is optional and must not become a dependency for the core system.
Prefer a replaceable, configurable local backend that is activated only when a task
contains that modality. Reuse an existing compatible model and load it on demand;
do not require extra always-resident weights or train a new model as a prerequisite.
If no suitable backend is configured, return a clear unsupported-modality result.

### Optional image understanding

An image adapter would accept bounded image input or a client-provided image
reference and, when enabled, ask a small configurable vision model to produce
structured observations for Jev-R. Depending on backend support, observations may
include a concise scene description, visible text/OCR, named objects and their
approximate locations, and a short list of visual details relevant to the user's
question. It should retain references to the source image and identify which
backend produced each observation. It should not silently claim exact measurements,
identity, intent, or facts that the image does not establish.

The observations are **untrusted evidence**, not instructions, verified facts or
permissions. Jev-R can compare them with other evidence and ask for clarification;
the Security Kernel continues to make all capability decisions. The adapter must
bound encoded bytes, decoded dimensions/pixels, preprocessing work, inference
tokens/time and output size; validate formats and reject malformed inputs. Raw image
bytes should not be copied into ordinary traces or SQLite events. Preserve a
privacy-conscious source reference/hash and provenance metadata, subject to an
explicit retention policy. If the user or client already supplies a description,
the system should be able to proceed without loading a vision model.

### Optional audio understanding

An audio adapter could produce a bounded transcript with segment timestamps and,
only when the selected backend supports it, speaker-turn labels. Preserve the audio
source reference and backend provenance. A transcript is a transcription result,
not proof that the speaker is correctly identified or that spoken instructions are
authorized. Bound file size, duration, sample rate/channels, decoding, inference
time and transcript length; keep audio bytes out of normal audit events.

### Tentative video understanding

Video is a later optional extension, after image and audio paths have independent
resource and trust controls. It may sample a capped number of frames at bounded
intervals, optionally pair them with audio-derived transcript segments, and return
timestamped observations with source references. It must not imply continuous
frame-by-frame analysis when it only sampled frames. Bound duration, resolution,
frame count/rate, audio decode, storage and inference budgets before processing.

For all modalities, external content and model-generated descriptions remain
untrusted and cannot grant network, filesystem, shell, connector or tool access.
Adapters should expose their cost and provenance to task accounting, allow the
operator to disable them, and have malformed-input, size-limit, timeout,
cancellation, privacy and policy-denial tests before being described as supported.

## Beyond the framework roadmap

New trained Jev neural architectures, claims of general reasoning superiority,
automatic active-weight changes and autonomous capability growth remain separate
research programs. They are not implied by protocol compatibility, modality
adapters or successful orchestration tests. Any such work needs independent
evaluation and the gates in the canonical architecture and threat model.
