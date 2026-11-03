# Actual Agent runtime acceptance: macOS arm64, 2026-11-03

Twelve actual Agent tools completed a read-only tool turn against Token Market
using the same selected model, `glm-5.3`, on macOS 27.0 arm64. Independent evidence
review linked each native call to its matching result, exact random nonce and
normal final completion. Eleven CLI tools have actual upstream SSE evidence;
WorkBuddy has native GUI chunk events, completed native tool records and an
independently observed completed conversation in its actual interface.

This is a bounded runtime result for client source snapshot
`e0578090d40237c39ca4907d96cb7d90d8576a3a`. It does not establish all models, other
native operating systems, every installation route, or full Connect/Disconnect
acceptance. The work remains unreleased. Issues #25–27 remain open.

## Method and evidence

Each run received a fixture filename and a request to read it. The file
contained a tool-specific random nonce that was absent from the prompt. Review
required the native read call, its corresponding completed result containing the
complete nonce, a final answer with that exact nonce, and the tool's normal
terminal event. Exit zero or a correct final answer alone was insufficient.
Codex performed its read through its native `exec_command` tool running `cat`.

The eleven CLI runs used a transparent forwarding observer for the real upstream
requests and responses. It did not mock the model, synthesize SSE or substitute
tool results. Review restricted requests to each native run's time window and
checked the selected model, HTTP 200, `text/event-stream`, text deltas in both
streams, tool deltas in the tool-selection stream, and timing against native
call/result/final events. Each CLI row below has two main generation streams:
tool selection followed by the final response.
Auxiliary/title requests are excluded from those counts.

Some headless CLI modes publish text only at step or message boundaries. Their
stdout event format alone does not prove token streaming; actual upstream SSE
is the transport evidence. Several tools also emitted native partial-text events
at distinct times. WorkBuddy was exercised in its actual GUI: expanded native
tool/result records and the displayed fixture, model and final result matched;
native `agent_message_chunk` events had two distinct timestamps before completion.
No CLI forwarding-observer result is claimed for that GUI conversation.

The compact [machine-readable summary](acceptance/2026-11-03-macos-runtime.json)
contains only tool/version/platform/model metadata, verification booleans and
stream/call summaries. It was derived from the independently audited runtime
matrix and measured versions. Raw transcripts, model reasoning, nonce values,
credentials, credential fingerprints, account/organization names and private
configuration paths are not included in this report or that artifact.

## Verified matrix

Every row passed native call/result linkage, the exact nonce in both result and
final, and normal completion. SSE counts are main-stream data frames before and
after the read result; they are not token counts or performance benchmarks.

| Tool ID | Measured version | Native read action / result linkage | Streaming evidence |
| --- | --- | --- | --- |
| `claude-code` | 2.1.309 | `Read`, matching `tool_use_id` | SSE 16 + 27 frames |
| `codex` | 0.166.0 | `exec_command` / `cat`, same completed item, exit 0 | SSE 23 + 14 frames |
| `workbuddy` | 5.9.3 | `Read`, matching native `callId` and GUI result | Native GUI chunks at 2 distinct timestamps |
| `deepseek-harness` | 0.2.1-rc.5 | `read`, matching completed `callId` | SSE 13 + 36 frames |
| `openclaw` | 2026.10.9 | `read`, matching `toolCallId`, no tool error | SSE 9 + 12 frames |
| `opencode` | 1.19.5 | `read`, matching completed call ID | SSE 41 + 9 frames |
| `grok-build` | 1.0.70 | `read_file`, matching `tool_use_id`, no tool error | SSE 32 + 27 frames |
| `hermes` | 0.22.5 | `read_file`, matching `tool_call_id`, no tool error | SSE 25 + 44 frames |
| `pi` | 1.4.0 | `read`, matching `toolCallId`, no tool error | SSE 10 + 12 frames |
| `minimax-code` | 0.7.2 | `read`, same native ID links input/completed output | SSE 28 + 25 frames |
| `kimi-code` | 2.4.1 | `Read`, matching `tool_call_id` | SSE 31 + 45 frames |
| `oh-my-pi` | 18.15.0 (`omp/18.15.0`) | `read`, matching `toolCallId`, no tool error | SSE 17 + 22 frames |

Hermes used source revision `bf0203a794a07aac3ff764972ab9087baa4e4f4e`.
WorkBuddy and Harness version provenance was the installed application metadata;
the other versions were explicitly measured from their launchers. Harness retains
the accepted fixed RC exception; it is not represented as a stable release.

## Isolated setup and remaining compatibility limits

The acceptance coordinator reused an existing platform Key and available model
catalogue. This proves these runtime paths, rather than new credential issuance or the
complete product connection lifecycle.
The eleven CLI runs used isolated HOME/profile environments. WorkBuddy's actual
GUI reused its existing connection and selected `glm-5.3`, with a new temporary
workspace and test conversation; its existing model/Key configuration was not
rewritten, and its entire profile was not isolated. The GUI test workspace and
conversation were retained as evidence. Existing user model preferences were not
rewritten. The independent runtime review did not inspect private profiles and
does not establish an unconfigured first launch or product default selection
behavior.

- Kimi used the same `glm-5.3` as a temporary default to satisfy its upstream
  authentication preflight before the explicitly selected runtime turn.
- Hermes was initialized with the same provider/model for first launch. Its
  final run also set the official
  `auxiliary.title_generation.model_upgrade_enabled: false` in the disposable
  profile. Rule-based titles remained; the separate automatic title-model
  upgrade request was disabled. Existing user model preferences were not rewritten.
- Grok's helper models selected the same GLM model. Historical requests for a
  different auxiliary model are outside the final passing window.
- Harness used a read-only preset. This does not validate every preset or
  permission mode.

Hermes's final independently reviewed run completed in 5.302 seconds with two
successful streams (25 + 44 frames) and five discovery GET 404s. A historical
post-turn non-streaming 400 disappeared after disabling title model upgrade. Its
timing and request shape were consistent with upstream deferred title upgrade,
but the exact task identity and server rejection cause were not proved. The
historical failure remains a limitation of that earlier setup; it is not counted
as a successful stream or presented as a current failure.

MiniMax retained an auxiliary input-token endpoint 404 despite its two successful
generation streams. Codex's missing-model-metadata/fallback warning, Claude's
startup backup warnings and OpenClaw's stale-lease/profile warnings were retained
in the evidence. A completed read turn does not establish warning-free behavior
or compatibility with every auxiliary endpoint.

## Installation defects found during acceptance

Three product defects were fixed separately from the runtime turns:

1. Healthy large downloads hit a 300-second curl ceiling and a 310-second outer
   deadline. The native downloader now keeps the connection timeout, detects
   sustained low-speed stalls, permits bounded transient retries and remains
   cancellable without a total healthy-transfer deadline. Existing checksum gates
   remain unchanged. Kimi completed a real download in approximately 384 seconds;
   Oh My Pi's 215 MB artifact completed in 1070.5 seconds. These observations
   exercise the former timeout boundary, not every dependency bootstrap path.
2. Ubuntu 22.04's procps 3.3.17 parsed an unseparated negative process-group
   operand in a way that signaled the parent and an unrelated peer during
   cancellation. A disposable, isolated Linux PID namespace reproduced it;
   adding `--` before the operand confined TERM to the intended group. The
   public real-curl cancellation regression now runs in an isolated helper
   group and verifies parent/peer preservation. This Linux process-control
   check is not Linux Agent runtime acceptance.
3. Oh My Pi's official version output is `omp/18.15.0`. Exact tool-specific
   recognition now covers installation verification, existing-install reuse and
   the strict incomplete-install guard. A second clean HOME reused the already
   verified official binary through PATH with the rebuilt product CLI, without
   another download or Agent configuration creation. Owned partial attempts
   still require repair; changed versions are not automatically overwritten.

Earlier failed attempts remain failures. Successful later runtime turns or
clean-environment reuse do not retroactively turn them into installation passes.

## Release boundary

This report does not close #25, #26 or #27. Windows and Linux native Agent
installation/runtime coverage, remaining install/restore and connection lifecycle
checks, and release decisions require their own evidence. Linux Docker runtime
acceptance was still in preparation for this report and is not counted here.

Consult [PR #28's current checks](https://github.com/Duqiyan2/token-market-cli/pull/28/checks)
for the checked revision and latest automated results. This report intentionally
does not freeze a volatile CI pass claim. Compilation, fixture tests and package
smoke checks remain distinct from the real Agent turns documented above.
