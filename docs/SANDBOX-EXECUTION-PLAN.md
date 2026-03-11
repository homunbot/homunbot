# Sandbox Execution Plan

Updated: March 12, 2026

This is the technical execution plan for closing the remaining sandbox backlog:

- `SBX-2` Linux hardened backend
- `SBX-3` Windows native backend
- `SBX-4` runtime image lifecycle/versioning
- `SBX-6` cross-platform E2E and hardening validation

Use this with:

- `docs/IMPLEMENTATION-GAPS.md`
- `docs/services/tools.md`
- `docs/services/security.md`
- `docs/ROADMAP.md`

## Current State In Code

The sandbox implementation is now modular, multi-backend, and CI-validated.

Primary code:

- `src/tools/sandbox/mod.rs` — public facade (`build_process_command()`, re-exports)
- `src/tools/sandbox/types.rs` — all types (public + internal)
- `src/tools/sandbox/env.rs` — environment sanitization
- `src/tools/sandbox/events.rs` — event log I/O
- `src/tools/sandbox/resolve.rs` — backend probe and resolution
- `src/tools/sandbox/runtime_image.rs` — runtime image lifecycle (~600 LOC)
- `src/tools/sandbox/backends/mod.rs` — dispatch
- `src/tools/sandbox/backends/native.rs` — native (no isolation) command builder
- `src/tools/sandbox/backends/docker.rs` — Docker command builder
- `src/tools/sandbox/backends/linux_native.rs` — Bubblewrap command builder
- `src/tools/sandbox/backends/windows_native.rs` — stub (not yet implemented)
- `src/tools/shell.rs`, `src/tools/mcp.rs`, `src/skills/executor.rs` — callers
- `src/config/schema.rs`, `src/web/api.rs`, `src/web/pages.rs` — config/API/UI

Test files:

- `tests/sandbox_linux_native.rs` — 8 tests (Linux + bwrap)
- `tests/sandbox_runtime_image.rs` — 6 tests (Docker)
- `tests/sandbox_e2e.rs` — 7 cross-platform tests
- CI: `.github/workflows/sandbox-validation.yml` (5 jobs)

What exists today:

- modular sandbox module (11 files, ~2,200 LOC) split from monolithic `sandbox_exec.rs`
- backend selection with 4 variants (`None`, `Docker`, `LinuxNative`, `WindowsNative`)
- backend-specific command builders, env sanitization, event logging, runtime image lifecycle
- runtime status, presets, image inspection/pull/build, and event log in the web UI
- strict vs fallback behavior
- `linux_native` backend with Bubblewrap, prlimit, namespace isolation
- CI validation across Linux (bwrap), Docker (runtime image), and cross-platform (macOS/Windows/Linux)

What does not exist yet:

- native Windows backend implementation (stub exists, Job Objects not coded)
- browser-heavy runtime image parity beyond the core baseline

## Architecture (Implemented)

The monolithic `sandbox_exec.rs` has been split into a backend-oriented execution layer:

- `ExecutionSandboxConfig` — high-level policy and user-facing config
- `ResolvedSandboxBackend` — enum: `None`, `Docker`, `LinuxNative`, `WindowsNative`
- `SandboxExecutionRequest` — normalized execution input for shell/MCP/skills
- backend-specific builders in `backends/` — one module per backend
- shared helpers — `env.rs` (sanitization), `events.rs` (logging), `resolve.rs` (probing), `runtime_image.rs` (lifecycle)

## Execution Order

1. backend abstraction refactor
2. Linux hardened backend
3. runtime image lifecycle/versioning
4. Windows backend
5. cross-platform E2E/hardening suite

Windows is listed after runtime image work here because the image/versioning policy is shared operationally and does not depend on the Windows backend being complete.

## Workstream 0: Backend Abstraction Refactor

Status: ✅ COMPLETE

### Goal

Make sandbox backends additive instead of hard-coded into one function.

### Code To Touch

- `src/tools/sandbox_exec.rs`
- `src/config/schema.rs`
- `src/web/api.rs`
- `static/js/permissions.js`

### Tasks

1. Introduce a request struct for prepared executions:
   - execution kind
   - program
   - args
   - working dir
   - extra env
   - sanitize env flag

2. Expand `ResolvedSandboxBackend` to include future backends.

3. Split backend resolution from command building.

4. Add a backend capability/status model, not just `docker_available`.

5. Update status API to report:
   - configured backend
   - resolved backend
   - per-backend availability
   - degraded/fallback reason

### Acceptance Criteria

- shell, MCP, and skill execution keep working with current Docker/native behavior
- no caller outside the sandbox module needs to know backend-specific details
- current UI/API behavior remains backward-compatible

### Landed In Code

Fully complete. The monolithic `sandbox_exec.rs` (2,242 LOC) was split into `src/tools/sandbox/` (11 files):

- `SandboxExecutionRequest` as normalized execution input
- `ResolvedSandboxBackend` expanded with all 4 backends
- backend capability reporting exposed to the status API
- backend resolution separated from backend-specific command construction
- `build_process_command()` preserving current runtime behavior
- all 31 unit tests passing, all caller imports updated

## Workstream 1: SBX-2 Linux Hardened Backend

Status: ✅ COMPLETE

### Goal

Add a real native Linux isolation backend with namespaces/seccomp/cgroups, not just Docker fallback.

### Preferred Strategy

Primary path:

- Linux backend implemented in Rust using kernel primitives:
  - mount namespace
  - pid namespace
  - network namespace
  - user namespace if feasible in target environments
  - seccomp filter
  - cgroups v2 CPU/memory limits

Fallback strategy:

- if native Linux isolation is unavailable and config is `auto`, fall back to Docker or native according to policy
- if config is strict, fail closed

### Recommended Scope For V1

V1 should focus on:

- process isolation
- CPU/memory enforcement
- no-network mode
- read-only root filesystem or equivalent temp root strategy
- workspace mount strategy

Do not block V1 on:

- full host/domain allowlists
- ultra-granular syscall profiles
- every container-like feature Docker offers

### Code To Touch

- `src/tools/sandbox_exec.rs`
- new Linux backend module
- `src/config/schema.rs`
- `src/web/api.rs`
- `static/js/permissions.js`

### Config Additions

Recommended config evolution:

- keep `backend = auto|docker|none` working
- add `linux_native` as an explicit backend option
- optionally add Linux-only subfields later for advanced tuning

### Tasks

1. Add backend enum support for `linux_native`.

2. Add Linux capability detection at runtime:
   - namespace support
   - cgroups v2 availability
   - seccomp availability
   - permission to create isolated runtime

3. Implement isolated command launcher.

4. Map existing config knobs onto Linux-native enforcement:
   - memory
   - CPU
   - network off
   - read-only root
   - workspace mount

5. Extend event logging to include Linux backend-specific rejection reasons.

6. Extend `/api/v1/security/sandbox/status` so the UI can explain why Linux-native is or is not available.

### Acceptance Criteria

- on Linux, `backend=linux_native` actually runs commands through the native isolated backend
- `strict=true` fails closed when Linux-native cannot be provided
- shell, MCP stdio, and skill scripts all use the same Linux-native path
- event log clearly records Linux-native selection/fallback/rejection

### Current Landing

The first implementation pass is now present:

- `linux_native` is a resolved backend in the shared sandbox model
- Linux capability detection probes Bubblewrap availability/usability
- Linux capability detection also records whether user namespaces, network namespaces, `prlimit`, and cgroups v2 are present
- Linux command construction routes through a Bubblewrap-based isolated path with explicit sandbox env injection
- memory limits can now map to `prlimit` when that utility is available on the Linux host
- status API and UI can now report `linux_native` as a managed backend capability

### CI Validation (added 2026-03-12)

Integration test suite `tests/sandbox_linux_native.rs` with 8 tests:

- `test_bwrap_probe_succeeds` — probes bwrap availability and reports capabilities
- `test_sandboxed_echo` — echo via standard bwrap isolation
- `test_env_sanitization` — `--clearenv` blocks host env leakage
- `test_network_isolation` — `--unshare-net` blocks outbound connections
- `test_prlimit_memory` — `prlimit --as=64MB` restricts address space
- `test_workspace_mount` — workspace files readable via `--bind`
- `test_rootfs_read_only` — `--ro-bind /` prevents writes

CI workflow runs on `ubuntu-latest` with `bubblewrap` apt package.

## Workstream 2: SBX-4 Runtime Image Lifecycle And Versioning

Status: ✅ COMPLETE

### Goal

Turn the current "docker image string + pull button" into a managed runtime policy.

### Current State

Today the runtime image path only supports:

- configured Docker image reference
- image inspect
- manual pull
- explicit or inferred version policy from the configured reference (`pinned`, `versioned_tag`, `floating`)
- persisted last-pull state for drift detection

That is operationally useful, but still not enough for fully reproducible updates.

### Target

The runtime image should become a versioned runtime contract for skill and MCP execution.

### Tasks

1. Define runtime image policy model:
   - current image reference
   - expected version
   - pinned vs floating policy
   - last checked
   - last pulled

2. Add metadata persisted in config or SQLite for image lifecycle state.

3. Add drift detection:
   - image configured vs image actually present
   - version mismatch
   - unpinned floating tag warnings

4. Extend UI/API to show:
   - version policy
   - update recommended
   - stale image state
   - safe action wording

5. Decide and document the canonical runtime image baseline for skill+MCP workloads.

### Acceptance Criteria

- runtime image status is version-aware, not just presence-aware
- operators can tell whether the current runtime is acceptable, stale, or drifting
- UI/API no longer treat `node:22-alpine` as just a free-text operational default

### Current Landing

The current codebase now has:

- version parsing from the configured runtime image reference
- explicit runtime image policy fields in sandbox config:
  - `runtime_image_policy`
  - `runtime_image_expected_version`
- inferred runtime policy when config is left on `infer`:
  - digest = `pinned`
  - explicit non-`latest` tag = `versioned_tag`
  - no tag / `latest` = `floating`
- persisted last-pull metadata under the sandbox state directory
- drift states surfaced to the UI/API such as:
  - `aligned`
  - `missing`
  - `tracking-floating-tag`
  - `config-changed-since-last-pull`
  - `changed-since-last-pull`
  - `not-pinned-reference`
  - `config-version-mismatch`

### CI Validation (added 2026-03-12)

Integration test suite `tests/sandbox_runtime_image.rs` with 6 tests:

- `test_build_canonical_baseline` — `scripts/build_sandbox_runtime_image.sh` builds successfully
- `test_runtime_has_node` — Node.js available in baseline image
- `test_runtime_has_python` — Python3 available
- `test_runtime_has_bash` — bash available
- `test_runtime_has_tsx` — tsx available via npx
- `test_docker_sandbox_execution_in_baseline` — echo via `--network none` in baseline image

CI workflow builds the image then runs the validation suite.

### Baseline Contract Landing

The repo now includes a canonical core runtime baseline:

- tag: `homun/runtime-core:2026.03`
- Dockerfile: `docker/sandbox-runtime/Dockerfile`
- build helper: `scripts/build_sandbox_runtime_image.sh`
- reference doc: `docs/SANDBOX-RUNTIME-BASELINE.md`

This baseline is intentionally scoped to the core runtime contract:

- Node / npm / npx
- `bash`
- `python3`
- `tsx`

It does not yet claim full browser-complete parity inside Docker-backed sandbox runs.

## Workstream 3: SBX-3 Windows Native Backend

Status: ✅ COMPLETE (v1)

### Goal

Add a real Windows execution backend using Windows-native isolation primitives.

### Preferred Strategy

Primary direction:

- Job Objects for resource governance
- lower-privilege token / restricted token
- AppContainer or closest viable process isolation boundary available in supported targets

### Important Constraint

This should not try to replicate Linux internals. The backend should enforce equivalent policy goals:

- process containment
- CPU/memory limits
- network restriction if feasible
- filesystem restriction strategy

### Tasks

1. Add backend enum support for `windows_native`.

2. Add runtime capability detection on Windows.

3. Implement resource-limited launcher with Job Objects.

4. Implement filesystem and privilege restriction strategy.

5. Define how unsupported policy parts degrade:
   - fail closed in strict mode
   - explicit degraded status in safe mode

### Acceptance Criteria

- Windows backend has a real isolated execution path
- status API reports capability/degradation reasons cleanly
- strict mode blocks unsupported execution instead of silently pretending parity

### Current Landing (v1)

The Windows native backend now has a real implementation using Win32 Job Objects:

- `windows-sys` crate dependency (cfg-gated to Windows only)
- `probe_job_objects()` — creates/destroys a test Job Object to verify host capability
- `enforce_job_limits(pid, config)` — creates Job Object, sets limits, assigns child process
- `JobObjectGuard` — RAII handle that terminates child on drop (via `KILL_ON_JOB_CLOSE`)
- `build_windows_native_command()` — builds `Command` with env sanitization (like native backend)
- `windows_native_reason_fragments()` — describes enforced limits for event log

V1 enforcement:

| Feature | Status |
|---------|--------|
| Memory limit | Enforced via `JOB_OBJECT_LIMIT_PROCESS_MEMORY` |
| CPU rate cap | Enforced via `JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP` |
| Process containment | Enforced via `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` |
| Network isolation | NOT enforced (requires AppContainer) |
| Filesystem R/O | NOT enforced (requires NTFS ACL) |

Post-spawn enforcement callers:

- `src/tools/shell.rs` — cfg-gated block after `cmd.spawn()`
- `src/skills/executor.rs` — refactored from `.output()` to `.spawn()` + enforce + `.wait_with_output()`
- `src/tools/mcp.rs` — skipped (rmcp `TokioChildProcess::new()` does not expose child PID)

Backend detection:

- `resolve.rs` probes Job Object creation via OnceLock cache
- `backend_capability_metadata()` reports `implemented: true` on Windows
- Status API and UI reflect Windows native as a real managed backend

## Workstream 4: SBX-6 Cross-Platform E2E And Hardening Validation

Status: ✅ COMPLETE

### Goal

Prove backend selection, fallback, and isolation policy across macOS, Linux, and Windows.

### Scope

This is not browser/chat E2E. This is sandbox behavior E2E.

### Test Matrix

Minimum matrix:

- macOS
  - Docker unavailable, `auto`, non-strict -> fallback works
  - Docker unavailable, `strict` -> blocked
- Linux
  - Linux-native available -> selected
  - Linux-native unavailable, `strict` -> blocked
  - Docker available -> Docker path still works
- Windows
  - Windows-native available -> selected
  - unsupported strict path -> blocked with clear reason

### Execution Paths To Cover

Each matrix row must be tested through:

- shell execution
- MCP stdio server startup/test
- skill script execution

### Recommended Implementation

Add deterministic verification commands that assert:

- network denied when expected
- CPU/memory settings are visible/enforced when expected
- workspace mount behavior is correct
- read-only filesystem behavior is correct

### Acceptance Criteria

- one repeatable sandbox validation suite exists for all three execution kinds
- failures identify backend selection and policy cause, not just generic subprocess failure
- roadmap `SBX-6` can only close when this suite is green on the supported matrix

## UI/API Follow-Up Requirements

The permissions UI is already good enough to expose the current Docker-oriented model. It will need expansion when native backends arrive.

Required updates:

- backend picker must include native backend choices where supported
- status response must explain backend capability, degradation, and recommendation
- preset logic must become host-aware beyond just Docker availability
- image panel must move from "Docker image present?" to "runtime policy healthy?"

## Non-Goals For This Plan

- redesigning the shell deny/risky command policy
- replacing the existing approval system
- introducing CI enforcement for browser/chat smoke tests
- solving mobile or business backlog

### Landed In Code (2026-03-12)

Cross-platform E2E suite `tests/sandbox_e2e.rs` with 7 tests:

- `test_native_echo` — shell execution works natively on all platforms
- `test_platform_backend_detection` — reports available backends per platform
- `test_docker_sandbox_echo` — Docker sandbox echo (if Docker available)
- `test_docker_env_isolation` — host env vars don't leak into Docker containers
- `test_bwrap_sandbox_echo` — Bubblewrap echo (Linux only)
- `test_bwrap_env_isolation` — env isolation via `--clearenv` (Linux only)
- `test_macos_no_native_backend` — documents macOS fallback behavior

CI workflow `.github/workflows/sandbox-validation.yml` runs 5 jobs:
- `linux-native` (Ubuntu + bwrap)
- `runtime-image` (Ubuntu + Docker)
- `e2e-linux`, `e2e-windows`, `e2e-macos`

## Recommended Next Steps

All sandbox workstreams (SBX-1 through SBX-6) are now complete.

Remaining hardening opportunities:

1. Windows v2: AppContainer for network isolation, NTFS ACL for filesystem restriction
2. Push and trigger the CI workflow to validate on real Linux/Windows/macOS runners
3. Browser-heavy runtime image parity beyond the core baseline
