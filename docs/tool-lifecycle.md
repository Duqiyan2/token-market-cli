# Tool display, installation, upgrades and uninstall

Policy reference: CC Switch [`e217bfc`](https://github.com/farion1231/cc-switch/commit/e217bfcbbbbbf82f4619ab62f08fb002635b7858), adopted at the user's request on 2026-11-04. This supersedes the earlier npm-first and fixed-version new-install decisions. The existing tool catalogue is retained; Gemini and Antigravity are not added by this work.

## Display and discovery

Group tools as installed or not installed. A discovered installation whose version probe fails is shown as “无法运行”, with its error and uninstall available. Do not turn that failure into a second installation. Show the current version and available update, the executable path and only recognized source badges. Unknown origins have no source badge; an executable's native format does not establish its historical installer.

Honor terminal PATH precedence instead of selecting npm or the newest version. On Unix the desktop obtains the login-shell PATH once with a three-second timeout; Windows includes registry PATH discovery. Common locations remain fallback discovery, and aliases are deduplicated by their resolved file. Explicit version checks inspect every candidate; passive filesystem reads do not run candidate tools. Mark a terminal default only when it was resolved, and otherwise distinguish a unique installation from an unconfirmed fallback. CLI launch, version checks and upgrades use this same resolver. Existing graphical applications remain supported; a detected CLI takes precedence over an optional CLI product's desktop bundle.

## New installations

| Tool | macOS / Linux | Windows |
| --- | --- | --- |
| Claude Code | Official script; npm on script failure | npm |
| Codex, OpenClaw, Pi, DeepSeek Harness CLI | npm | npm |
| OpenCode | Official script; npm on script failure | npm |
| Grok Build, MiniMax Code | Official script; npm on script failure | Official PowerShell script; npm on script failure |
| Hermes, Kimi Code, Oh My Pi | Official script | Official PowerShell script |

npm uses the official global package's `latest` channel and existing Node/npm. Do not prepare the earlier tool-private Node runtimes or change the global npm prefix. Preserve optional dependencies and allowed install scripts for Claude, Grok, OpenCode and MiniMax; DeepSeek Harness uses `--engine-strict`. Download POSIX installers to a temporary file, run them only after successful download, retain failures and clean up temporary files. Captions derive from the same executable plan.

WorkBuddy and DuMate are additional graphical products without a corresponding CC Switch CLI rule; they retain their official native installers. QwenWork retains its existing official manual installation entry. Their native installers and application update UI own their install/update operations.

Installation and upgrade do not connect, disconnect or test a model. Latest installation versions are not a statement of model-adapter compatibility; connection checks remain separate.

## Upgrades

Prefer the current installation's official self-updater or its package manager. Homebrew ownership is handled before inspecting embedded node_modules. Recognized official Codex, MiniMax, Claude and Grok layouts retain their target directories. Node-manager installations use their corresponding absolute manager; verified global npm installations retain their prefix. Kimi before 0.43.0 uses a verified package manager or the official installer instead of its ineffective noninteractive `upgrade --yes`. Windows OpenCode uses the package-manager route instead of its interactive `upgrade` command. Unknown native installations outside node_modules remain manual updates.

Display each installation's path, version, runnable state and terminal-default marker. Multiple installations require confirmation even if their versions agree; only version/runtime disagreement is a conflict. Show the upgrade target path and generated command. If a default cannot be confirmed, say so before the generic fallback. The backend accepts a release version and a plan revision, not executable text. Recheck the path, channel, plan and current version under the tool lock, refusing stale confirmation and downgrades. Batch confirmation retains each selected plan.

Read `engines.node` for the target npm version when the selected installation runs on Node; report an identified runtime mismatch and check it again before execution. Unknown metadata/ranges remain advisory. Verify the actual installed version after execution; an exit code of zero with an unchanged/outdated or unreadable version is a failure.

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
