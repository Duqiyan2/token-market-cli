# Tool display, installation, upgrades and uninstall

Policy reference: CC Switch [`e217bfc`](https://github.com/farion1231/cc-switch/commit/e217bfcbbbbbf82f4619ab62f08fb002635b7858), adopted at the user's request on 2026-11-04. This supersedes the earlier npm-first and fixed-version new-install decisions. The existing tool catalogue is retained; Gemini and Antigravity are not added by this work.

## Display and discovery

Group tools as installed or not installed. A discovered installation whose version probe fails is shown as “无法运行”, with its error and uninstall available. Do not turn that failure into a second installation. Show the current version and available update, the executable path and only recognized source badges. Unknown origins have no source badge; an executable's native format does not establish its historical installer.

Honor terminal PATH precedence instead of selecting npm or the newest version. On Unix the desktop obtains the login-shell PATH once with a three-second timeout; Windows includes registry PATH discovery. Common locations remain fallback discovery, and aliases are deduplicated by their resolved file. Explicit version checks inspect every candidate; passive filesystem reads do not run candidate tools. Mark a terminal default only when it was resolved, and otherwise distinguish a unique installation from an unconfirmed fallback. CLI launch, version checks and upgrades use this same resolver. Existing graphical applications remain supported; a detected CLI takes precedence over an optional CLI product's desktop bundle.

## New installations

Windows installation preference (user decision, 2026-11-06): prefer an official
EXE/MSI installer, then an official standalone executable or ZIP for the matching
product and architecture. Use npm or another required package manager only when
no suitable official native distribution is available. An installed Node/npm
runtime does not give npm priority. An official script that installs the native
distribution belongs to that native route. Preserve existing installations and
their update ownership. This decision supersedes conflicting Windows entries in
the channel table below. Windows routing now uses the official
Harness EXE (x64), OpenCode ZIP (x64), Kimi/Pi/Oh My Pi standalone releases,
and native installer scripts for Claude Code, Codex and Grok Build. OpenClaw
and platforms without a supported native route retain their package-manager
channel. MiniMax retains its official script with npm fallback; Hermes retains
its official script. Standalone archives use the verified versions and checksums
in their recipes; native scripts retain their official current channel. Verify the destination executable and its version, then
refresh discovery before reporting installation success.

| Tool | macOS / Linux | Windows |
| --- | --- | --- |
| Claude Code | Official script; npm on script failure | Official native PowerShell installer |
| Codex | npm | Official native PowerShell installer |
| OpenClaw | npm | npm (no selected native distribution) |
| Pi | npm | Official standalone ZIP |
| DeepSeek Harness | npm | Official desktop EXE on x64; npm on arm64 |
| OpenCode | Official script; npm on script failure | Official V1 CLI ZIP on x64; npm on arm64 |
| Grok Build | Official script; npm on script failure | Official native PowerShell installer |
| MiniMax Code | Official script; npm on script failure | Official PowerShell script; npm on script failure |
| Hermes | Official script | Official PowerShell script |
| Kimi Code, Oh My Pi | Official script | Official standalone release |

npm uses the official global package's `latest` channel and existing Node/npm. Do not prepare the earlier tool-private Node runtimes or change the global npm prefix. Preserve optional dependencies and allowed install scripts for Claude, Grok, OpenCode and MiniMax; DeepSeek Harness uses `--engine-strict`. Download POSIX installers to a temporary file, run them only after successful download, retain failures and clean up temporary files. Captions derive from the same executable plan.

WorkBuddy, DuMate and QwenWork are additional graphical products without a corresponding CC Switch CLI rule; they use their official native installers. QwenWork downloads the official latest DMG for macOS arm64/x64 or EXE for Windows x64, then opens it with the native OS handler. Opening an installer does not mark the application installed; discovery must find the actual application. Installation does not connect or require login; users log in before the separate connection step. Existing QwenWork installations are reused without automatic upgrades or downgrades, and SDK compatibility remains a connection preflight check. Their native installers own actual installation. QwenWork additionally offers the same public latest-channel download for an explicit update, without requiring login, database initialization or connection compatibility. It does not advertise an unverified new version number. The existing background download/handoff behavior is unchanged, with no new handoff popup or reconnect prompt on download completion.

Desktop installation follows CC Switch's per-tool busy button and background task pattern ([button](https://github.com/farion1231/cc-switch/blob/ca58c0313360eae7c851fbd7553c1614c7af03e7/src/components/apps/AppsPage.tsx#L780), [task store](https://github.com/farion1231/cc-switch/blob/ca58c0313360eae7c851fbd7553c1614c7af03e7/src/components/apps/useToolManagement.ts#L657)). Starting a download does not open a progress modal or lock other tools. Direct downloads expose actual downloaded bytes and known total size inline; details and cancellation remain available from the tool details page. Graphical downloads automatically open their native installer, then release the desktop task and restore Install so the user can download again. Opening an installer is not installation success. CLI installations finish in the background and refresh actual discovery; returning from a native installer refreshes the page through activation/navigation events. Terminal CLI installation retains its existing wait-for-application behavior.

Installation and upgrade do not connect, disconnect or test a model. Latest installation versions are not a statement of model-adapter compatibility; connection checks remain separate.

## Upgrades

Prefer the current installation's official self-updater or its package manager. Homebrew ownership is handled before inspecting embedded node_modules. Recognized official Codex, MiniMax, Claude and Grok layouts retain their target directories. Node-manager installations use their corresponding absolute manager; verified global npm installations retain their prefix. Kimi before 0.43.0 uses a verified package manager or the official installer instead of its ineffective noninteractive `upgrade --yes`. Windows OpenCode uses the package-manager route instead of its interactive `upgrade` command. Unknown native installations outside node_modules remain manual updates.

Display each installation's path, version, runnable state and terminal-default marker. Multiple installations require confirmation even if their versions agree; only version/runtime disagreement is a conflict. Show the upgrade target path and generated command. If a default cannot be confirmed, say so before the generic fallback. The backend accepts a release version and a plan revision, not executable text. Recheck the path, channel, plan and current version under the tool lock, refusing stale confirmation and downgrades. Batch confirmation retains each selected plan.

Read `engines.node` for the target npm version when the selected installation runs on Node; report an identified runtime mismatch and check it again before execution. Unknown metadata/ranges remain advisory. Verify the actual installed version after execution; an exit code of zero with an unchanged/outdated or unreadable version is a failure.

## Connection compatibility after installation changes

New connections and reconnections save the resolved installation's passive identity and any locally observed version in their existing encrypted connection record. Startup, activation, navigation and explicit update checks compare actual installations with that baseline, including changes made through a vendor client, website installer, downgrade or reinstall. Remote release-check failures do not prevent local revalidation. Historical records without a baseline get a first-observation baseline after their existing configuration passes local checks. This metadata-only migration updates the encrypted connection record without changing tool configuration or credentials; the first observation alone does not produce an upgrade warning. If local checks cannot establish a baseline, compatibility remains unknown.

Connection compatibility is independent of release availability and installation-job completion. Revalidation does not change configuration, issue credentials, start SDK workers, or send a model request. It reuses configuration ownership/parsing and explicit adapter restrictions: OpenCode 2+ and legacy Kimi are rejected by their existing rules; DuMate Personal checks its installed engine capabilities; QwenWork checks its packaged loading contract, SDK structure and database schema, then compares the SDK source fingerprint with the external runtime. Generic configuration adapters retain an unchanged configuration within a recorded major family; an unverified new major remains unknown. These are local adapter checks, not real-task or billing acceptance.

A compatible connection keeps its normal list state. A supported adapter whose external assets need refresh displays “需要重新接入”. A known incompatible installation displays “当前版本暂不支持” with administrator guidance. Inconclusive compatibility (including unreadable metadata, an unrecorded baseline or an unverified major) preserves the existing connection state and available actions, without warnings, reconnects or configuration changes after installation or online updates. Assessments are bound to the connection ID so an old result cannot override a later reconnect. Only the user-triggered reconnect refreshes its adapter and baseline.

For native QwenWork downloads, reaching the existing internal handoff terminal releases the task and restores the update button. It neither waits for installation nor claims upgrade success. The versionless download action is not counted as a confirmed newer release or included in the batch of confirmed upgrades. Native handoffs in a batch, if any, are counted as downloaded rather than installed.

## Uninstall

“更多 → 卸载” first enumerates individual candidates. A unique automatic candidate may be preselected; multiple installations require selection. Preview the selected path, source where known, exact generated operation and files to remove. Unknown ownership stays manual with official guidance.

- Homebrew: verify the tool-specific formula/cask owner and original brew executable. Never remove an arbitrary token inferred only from a path.
- npm: verify the official package name, canonical package directory, global launcher layout and ownership of the actual executable or Windows shim. Use an absolute npm and explicit original prefix. A sibling manager or source badge alone is not ownership evidence; local project `.bin` entries are not global installations.
- Hermes: selected CLI `uninstall --yes`, without `--full`.
- OpenClaw: remove only its Gateway service with `uninstall --service --yes --non-interactive` before the package uninstall; retain workspaces, sessions and configuration.
- Known native Claude, Codex, Grok, OpenCode and Oh My Pi layouts: remove only the validated executable and its aliases. Grok additionally removes specifically named installer binaries/history and its verified `agent` alias. Never recursively delete their installation/data directories.
- Other native layouts, unverified Bun/pnpm/Volta ownership and graphical applications: use the original installer, package manager or system application management.

Preview is read-only. Execution takes the shared lifecycle lock, re-enumerates the chosen path, regenerates the entire plan and compares it with the confirmed plan/revision before any mutation. A changed symlink, package identity, file or operation requires a fresh confirmation. Execute backend-generated operations only. Finally verify the selected directory entry is absent with symlink metadata; a dangling shim or manager exit code alone is not success. Report whether other installations remain. Configuration, conversations, credentials and connection receipts are retained.

## Verification

Tests use temporary files, mocked installers/package managers and an offline fake downloader. Actual POSIX shell execution verifies script success/failure, npm fallback, argument quoting and configuration retention. Ownership, stale plans, native aliases, historical Grok binaries, original npm prefixes, Windows shim identity and negative uninstall results have isolated regression coverage. Browser tests use mocked native IPC. These are not Windows real-device installation acceptance or upstream latest Agent/model acceptance.
