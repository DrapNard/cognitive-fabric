# Optional Linux managed-worker limits

Architecture sections 4, 9 and 10 require independent resource control. The
logical scheduler still uses declared allocations. This separate opt-in mechanism
adds kernel bounds to Cognitive Fabric's own managed worker processes.

## Configure

```sh
cogfabric --home ./state configure-worker-limits \
  --address-space-bytes 8589934592 --cpu-seconds 60
```

The example permits at most 8 GiB of virtual address space and 60 CPU seconds for
each new managed worker process. It is an example, not a measured minimum or an
RSS reservation. Choose bounds appropriate to your host and workload. Restart
existing services to load the changed configuration. Existing admitted tasks retain
their original settings; lowering a saved limit does not retroactively change a
running process.

Enable the desired worker mechanism separately, for example:

```sh
cogfabric --home ./state configure-worker --enabled true
# Or execute the reviewed typed tool module in its own process:
cogfabric --home ./state --typed-module-worker ask 'calc: 6*7' --mode fast
```

Clear the opt-in limits with:

```sh
cogfabric --home ./state configure-worker-limits --clear
```

The serialized configuration field is optional:

```json
{
  "worker_process_limits": {
    "address_space_bytes": 8589934592,
    "cpu_seconds": 60
  }
}
```

Accepted bounds are 64 MiB–1 TiB of address space and 1–3600 CPU seconds. Enabled
limits are supported only on Linux; unsupported platforms reject them. Omission
preserves existing behavior and does not add a configuration field to legacy JSON.
Only the human CLI has the configuration capability. Model output, provider
responses and chat requests cannot change these settings.

## Enforcement and scope

Before `exec`, the spawning child applies `RLIMIT_AS` and `RLIMIT_CPU`, clamps both
soft/hard values to no greater than the requested or inherited limits, and disables
core dumps. A setup syscall failure aborts spawn. The parent process is not modified.
The same immutable configuration reaches native conversation, inference,
arithmetic, SQL and compiled typed-module workers. Optional launch/ownership audit
metadata records the requested bounds. An inherited tighter cap can reduce the
actual value; audit metadata is not a readback of host privileges or a measured
resource-usage sample.

These are **per-process limits**, not a task-wide cumulative CPU counter or a
cgroup. CPU seconds are accumulated processing time, not wall-clock time. Address
space is virtual memory, not resident RAM. Excessively low limits can prevent
runtime/thread/SQLite startup or kill a worker; the task then follows its existing
failure and child-reaping path. The existing wall-time, queue and call budgets
remain independent.

See the [Linux resource-limit documentation](https://man7.org/linux/man-pages/man2/getrlimit.2.html)
for kernel semantics. These bounds do not constrain a separate LM Studio/llama.cpp
server, GPU memory, GPU time, energy, host RSS, files or network. They are not a
complete untrusted-code sandbox; privileged host code may bypass ordinary local
restrictions. The reviewed fixed worker remains trusted code.

## Pinning and recovery

Task configuration snapshots preserve the original limits for managed inference
and tools. Typed modules additionally bind enabled limits into their implementation
identity; task binding and registry reconstruction retain that exact identity.
Malformed or unsupported recovery settings are refused. Legacy snapshots without
an implementation identity retain the original no-opt-in configuration.

## Verification

Focused Linux subprocess regressions confirm readback, failure of an oversized
mapping, termination of a busy child, preservation of stricter inherited limits,
unchanged parent limits and rejection of malformed configuration. The ignored
child fixture is explicitly invoked by those parent tests; it is not a skipped
integration claim. Configuration/permission and exact typed-registry recovery
regressions pass. Seven native-worker tests run with limits enabled, and the
executable-replacement test covers bounded inference, arithmetic and SQL workers.
A fresh temporary CLI workspace also completes a typed arithmetic task, records
the limits, reaps its worker and clears the configuration successfully.

This verification does not establish physical GPU quotas, task-attributed energy,
independent model quality or full release readiness.
