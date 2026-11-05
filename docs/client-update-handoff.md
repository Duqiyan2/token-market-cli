# Guarded Client Update handoff

The native `ClientUpdater::install` operation accepts only the consent version.
The updater retains the verified candidate and bytes; IPC never accepts a package
path, URL, signature or bytes. The macOS Apple Silicon executor registers only for supported installed app
locations, with its concrete synchronous lifetime documented in
[macOS Client Updates](macos-client-updates.md). Other platform executors must
validate the following contract before calling `with_installer`. Unsupported
locations/platforms keep `installAvailable` false and direct commands refuse them.

## Shared protection

`tokenmarket_core::client_update::ClientUpdateGuard::current()` acquires the
existing connection operation lock, every Tool Installation lock and the current
configured plus already existing Client Session scope locks. `acquire(home,
origin)` is the same boundary with an explicit native home/origin for a guardian
or isolated tests. The home is the OS user home, not `.tokenmarket` itself.
Paths and data locations remain compatible with independently installed CLI
versions. DuMate Enterprise and Personal share one deduplicated installation
lock. Paths are sorted, locks are nonblocking, and any failure releases all
partial acquisition. Active operations finish normally; the user retries
explicitly. No cancellation or process termination is involved.

Keep the entire guard alive from the successful idle decision through known
installer termination. A new global gate or a journal marker cannot exclude an
old CLI which only understands its operation locks. Enumeration covers the
current service origin and existing login scopes. A legacy CLI creating a
previously nonexistent arbitrary origin uses a new namespace, outside this
finite lock set; do not claim exclusion for all possible future origins.

## Platform seam and ownership

`ClientUpdateInstaller::prepare(VerifiedClientUpdate)` obtains protection and
returns an owned `ClientUpdateHandoff`. Returning is the guarded idle decision.
No installer runs before `commit`; dropping a prepared handoff must abort safely.
`commit` returns a typed `ClientUpdateCommit::Handoff` or
`HandoffWithWarning` after commitment. A warning preserves pending handoff and
recovery guidance without enabling another attempt. An `Err` is retryable only
when installation is known inactive. An uncertain postcommit result remains pending in the journal
and must not return a retryable error. It must preserve the
lock owner across the whole installation interval. The high-level updater
serializes requests, retains verified bytes after refusal/known launch failure,
and reports `handoff`, never completed installation, after success.

For synchronous macOS installation, the prepared object can own the guard
itself and retain it through install termination before restart. Implement and
validate restart/journal reconciliation separately.

For Windows, a guardian **must acquire the full legacy lock set itself** before
acknowledging READY. Do not acquire in the desktop, release, then reacquire in a
helper: that introduces a race. Windows locks are process-owned, and desktop
exit releases them even if a Rust guard was not dropped or handles were
inherited. The prepared handoff represents the live guardian's READY response,
not a parent-owned guard. Before COMMIT, cancellation/disconnection aborts. After
COMMIT, the guardian waits for the exact parent process, launches the trusted
installer, keeps its process handle and all locks until known installer
termination, then records the result and relaunches the expected application.
A timeout or missing parent PID does not prove completion. Do not use the updater
plugin's exiting Windows install shortcut as this protocol.

A forcibly terminated guardian loses OS locks; an old CLI cannot honor the
journal. This guard does not promise exclusion against arbitrary guardian
killing. Actual Windows helper/process/installer lifetime tests belong to the
platform implementation before it can be enabled.

## Durable journal

`ClientUpdateJournal` stores only nonce, original and target version, native
application path and phase in `.tokenmarket/client-update-attempt.json`.
The guard-owning process serializes all writes. Writes are private, atomic and
flushed before the transition can be acknowledged. `begin` refuses to overwrite
an unresolved attempt; no elapsed TTL clears it. Read errors require recovery.

The reusable transition API is `Preparing -> Guarded -> Committed ->
InstallerRunning -> AwaitingStartup`, with known failure recorded as `Failed`.
Record `Guarded` before READY, `Committed` before acting on COMMIT and
`InstallerRunning` only after trusted process creation. `AwaitingStartup` is a
fact asserted by the platform only after known installer termination. Persist a
known nonzero installer result as failure; the journal API itself does not infer
an exit result or discover a running process. Windows must extend its native
attempt data with the trusted installer/process identity needed to reconcile
an interrupted guardian before enabling this path.

Only `acknowledge_startup(compiled_version, current_exe)` from the actual new
process can record `Succeeded`, and only after `AwaitingStartup` with matching
target version and original native application path. Installer launch, zero exit
code or restarting an old executable is insufficient. The macOS executor reconciles this journal during early native setup, even when
distribution configuration is blank. Platform launch/installation must not bypass
that actual-version startup acknowledgment.

Automated fixtures prove legacy lock conflicts in both directions, partial
release, explicit retries and native/UI handoff behavior. They are not platform
installation, Windows execution, code-signing or actual-device evidence.
