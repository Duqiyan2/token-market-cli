# Linux arm64 acceptance: 2026-11-03

Ten actual Agent tools completed an independently reviewed read-only tool turn
using `glm-5.3` in the Linux arm64 container. Codex's native read action is blocked
by the container sandbox environment. All eleven applicable CLI tools completed
product installation and version verification. WorkBuddy is unsupported on
ordinary Ubuntu.

This report records Linux installation evidence separately from actual Agent
runtime evidence. The tested product CLI is source snapshot
`e0578090d40237c39ca4907d96cb7d90d8576a3a`, CI run 41570682107,
`npm-native-linux-arm64`, version 1.1.1. Its SHA256 is
`a732bb28e31b61c96f38037af4a4f1ace2ac2832f18250989f9a8bc96582c4f4`,
verified against the artifact manifest before execution.

The [machine-readable summary](acceptance/2026-11-03-linux-runtime.json) records
the environment, artifact provenance, installation histories and reviewed runtime
gates. It excludes raw transcripts, model reasoning, nonce values, credentials,
credential fingerprints, private account/organization names and private profile paths.

The independent auditor completed the ten-tool matrix before its turn ended;
the coordinator also cross-checked the final Hermes native events and upstream
stream metadata. Final report editing and cleanup were coordinator-reviewed;
no product code changed during this documentation step. This report does not close #27. Windows native acceptance remains
explicitly deferred because no Windows environment is available.

## Environment and isolation

The environment is an ordinary Ubuntu 22.04.5 LTS arm64 Docker container on the
local Docker Desktop Linux VM, kernel 6.12.76-linuxkit, glibc 2.35. Installations
execute as uid 501/gid 20, without root privileges. The official cached Ubuntu image is
`sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11`.

The task owns its container and one temporary bind directory. It has a 4 GiB
memory limit and three CPUs, with ordinary bridge networking. No host PID/network
namespace, Docker socket, host user configuration, published port, privileged
mode or added capability is used. The host bind contains disposable test homes
and evidence. The coordinator owns credential injection and cleanup. Its private
input Key file resides outside that bind; disposable runtime profiles are also
treated as private. The installation worker does not inspect either, supply
provider credentials to installer processes, or perform model calls.

Installers receive a clean environment and a separate HOME for each tool. The
OpenClaw installer bootstraps its own official Linux Node 24.22.1, which subsequent
installers may reuse through an explicit PATH. No macOS binary is reused.
Hermes source preparation ultimately fetched the exact official pinned commit
`bf0203a794a07aac3ff764972ab9087baa4e4f4e` through a fresh depth-one HTTPS/HTTP1.1
fetch. This is recorded separately from its subsequent product dependency and
launcher installation. Earlier incomplete public-cache reuse is documented below.

## Installation and version evidence

The native product `install` command completed with exit zero for the following
eleven tools. Each installed launcher also returned exit zero for `--version` in a
credential-free environment. Elapsed times measure the observed install attempt,
including network transfer; they are not performance benchmarks. The table uses
terminal successful attempts. Retries may reuse prepared dependencies or public
caches and do not establish a completely cold bootstrap for every tool.

| Tool ID | Measured version | Install elapsed seconds |
| --- | --- | ---: |
| `claude-code` | 2.1.309 | 761.337 |
| `codex` | 0.166.0 | 260.148 |
| `pi` | 1.4.0 | 491.821 |
| `opencode` | 1.19.5 | 260.820 |
| `kimi-code` | 2.4.1 | 249.956 |
| `oh-my-pi` | 18.15.0 (`omp/18.15.0`) | 407.056 |
| `openclaw` | 2026.10.9 | 246.114 |
| `grok-build` | 1.0.70 | 60.298 |
| `minimax-code` | 0.7.2 | 41.959 |
| `deepseek-harness` | 0.2.1-rc.5 | 219.801 |
| `hermes` | 0.22.5, pinned source `bf0203a794…` | 205.507 |

Harness retains the explicitly approved fixed official RC exception; it is not
described as stable. Its npm installer emitted an `allow-scripts` warning, which
is retained separately from version verification and runtime acceptance.

Hermes's first installation attempt failed while fetching the public source
checkout, before dependency installation. Its reused public cache lacked partial
clone metadata after test preparation, leaving 206,476 missing blobs on `git
fsck --full` (exit 3). This is a test-preparation defect, not an established
product defect or successful fresh network bootstrap. The incomplete public cache
and failed history are preserved. A new official HTTPS/HTTP1.1 shallow fetch of
the exact pinned commit completed in 103.051 seconds and passed `git fsck --full`
with exit zero.

The next product attempt completed source/dependency preparation but failed when
the official managed Python archive encountered a terminfo symlink with errno 40
on the macOS-backed bind. A task-owned scratch probe confirmed that the bind is
case-insensitive and reproduced the case-alias self-loop; the same probe on the
container's native Linux filesystem passed. Only Hermes's official public
managed-tool storage was moved onto that case-sensitive Linux filesystem, with a
symlink preserving its original path. Profiles and Keys were not moved or read,
and no product source, privilege or sandbox setting changed. The failed attempt
and filesystem evidence are retained. This filesystem limitation is distinct
from Codex's user-namespace sandbox blockage.

A subsequent native product attempt was rate-limited (HTTP 429) while downloading
the same fixed official installer script, before it reached the relocated tools.
A single later public HEAD request returned 200 with no Retry-After header. The
worker retained the failed attempt and waited before one normal product retry;
it did not replace the official URL, version, checksum or product download stage.
That final product attempt completed with exit zero in 205.507 seconds. Its
launcher returned the expected 0.22.5/version source pin, with an unchanged
`Update available` warning; no upgrade was run. Its public managed-tool payload
was preserved as a 304,601,656-byte tar archive, SHA256
`bccc97c72f7a865afb278d8c88591b54c0cbe4803f5c9ec0390bedc2d4414c23`,
which retains Linux case/symlink semantics and contains no profiles or Keys.
Its later actual runtime turn also passed the separate independent audit.

Docker Desktop became unavailable during earlier OpenClaw/Grok/MiniMax/Harness
attempts; its daemon error is an infrastructure
interruption, not evidence that any individual upstream installer failed. The
existing logs and status files are retained. The coordinator's Docker diagnosis
recorded a filesystem event injection blocked for 60 seconds, followed by engine exit 255 with
`oom=false`; this is not evidence of an out-of-memory failure or a concurrency
root cause. An ordinary restart restored the same container. The interrupted
attempts were archived and only unfinished installations resumed, one at a time.
No successful installation is claimed from a stale `running` status.

The actual product `install workbuddy` command returned exit one immediately
with the expected ordinary-Linux unsupported explanation and official UOS/Kylin
application-store guidance. It is recorded separately from tools awaiting a
supported Linux install. Gemini CLI and Antigravity CLI remain outside this
implementation scope, per the user's prior decisions.

## Real cancellation and retry

OpenClaw's initial installation was deliberately cancelled during its npm stage,
after its private Node dependency was ready. A normal SIGINT targeted the single
native installer client. It returned the expected cancellation message in 0.733
seconds, with exit one. Node still reported v24.22.1 afterward; a live peer and
the concurrent Claude/Pi installers remained alive. This directly exercises the
fixed Unix process-group cancellation boundary without signaling unrelated
processes.

A new native OpenClaw install attempt then started against the same disposable
home and skipped the already prepared Node dependency. Its first cancelled
attempt remains a cancellation, and is not counted as an install success. The
retry completed with exit zero after infrastructure recovery. Its own version
probe also succeeded. All histories are retained.

## Runtime and remaining limits

The coordinator uses the existing platform and selected model `glm-5.3`. Each
passing run had a native read call linked to its completed result, an exact
tool-specific random nonce in that result and the final answer, and normal native
completion. The nonce was absent from the prompt. Independent review correlated
the native events with transparent forwarding-observer evidence for actual
upstream HTTP 200 SSE responses in the run's time window. No model response or
tool result was mocked. Exit zero and a correct final answer alone are
insufficient.

The runtime fixtures were prepared by the coordinator. Their core provider,
model alias and wire model were checked against the product adapter shapes;
this does not mean every profile field was written through product Connect.
Fixture-only initial selections include Kimi's default model for its upstream
authentication preflight and Hermes's provider/default-model initialization and
disabled auxiliary title-model upgrade. They do not establish an empty-HOME
first launch, unconfigured initial model selection, product default-selection
behavior or the complete Connect/Disconnect lifecycle. The product does not gain
authority from these fixtures to overwrite an existing user's default selection.

| Tool | Native read action | Main upstream SSE data frames | Result |
| --- | --- | --- | --- |
| Claude Code | `Read`, matching `tool_use_id` | 18 + 12 | Passed |
| Pi | `read`, matching `toolCallId` | 11 + 14 | Passed |
| OpenCode | `read`, matching completed call ID | 21 + 9 | Passed |
| Kimi Code | `Read`, matching `tool_call_id` | 23 + 26 | Passed |
| Oh My Pi | `read`, matching `toolCallId` | 24 + 10 | Passed with transport-close warning |
| OpenClaw | `read`, matching `toolCallId` | 10 + 10 | Passed |
| Grok Build | `read_file`, matching `tool_use_id` | 31 + 47 | Passed |
| MiniMax Code | `read`, matching completed native call ID | 30 + 42 | Passed with auxiliary endpoint warning |
| DeepSeek Harness | `read`, matching completed `callId` | 33 + 10 | Passed |
| Hermes | `read_file`, matching `tool_call_id` | 35 + 19 | Passed with discovery endpoint warnings |
| Codex | `exec_command`; native commands failed before reading | Four model SSE exchanges | Environment blocked |

Frame counts are observed SSE data frames for tool selection and the final
generation, not token counts or performance benchmarks. OpenCode's separate
auxiliary stream and Grok's title helper stream are excluded. Hermes retained
five metadata-discovery GET 404s before its two successful main streams, with no
trailing non-stream 400 in this final window. OpenClaw's existing
profile/tool-section warning remains recorded. MiniMax's auxiliary input-token
endpoint returned 404 despite its successful main streams. Oh My Pi's second
stream recorded an observer
`ConnectionResetError` despite complete native read/result/final/terminal events
and received usage metadata. Its exact transport-close cause is unproved; the
passing native turn does not establish an error-free transport window.

The initial OpenCode/Kimi/OMP runtime attempts were interrupted by the Docker
backend outage. They are not retroactively counted as successful; the table uses
only completed reviewed runs after recovery. Installation exit zero and
`--version` do not establish a successful tool turn, streaming transport or
product connection lifecycle.

The Linux Codex runtime encountered a default Docker user-namespace/bwrap
restriction while attempting its native read action. A successful model stream
does not make that read action pass. No privileged container, extra capability,
sysctl change or sandbox bypass is used to obtain a passing result.

This container is not a full native desktop host. GUI interaction, an unrestricted
Linux host sandbox, other platform/architecture installation routes, restoration and
Connect/Disconnect lifecycle checks require their own evidence. #25–27 remain
open; no release, publication or deployment is performed by this acceptance run.

## Cleanup

After the final runtime audit, the coordinator removed 16 disposable profile or
session paths and the container-only Key file, then stopped and removed the
task-owned container and its forwarding observer. Existing user profiles were
not modified by cleanup. The original fixture bytes and single-file directories
remained unchanged for all eleven applicable CLI fixtures.

A completed scan of 191,822 regular files (6,872,971,224 bytes) in the task-owned
host directory found no remaining plaintext copy of the reused Key and no read
errors. It did not follow symlinks. Public installation data, the public-only
managed-tools archive and sanitized evidence were retained; this is not a claim
that the disposable runtime environments remain executable after cleanup.
