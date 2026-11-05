# Desktop client updates

Product design confirmed on 2026-11-05. The decisions below describe intended
behavior, not an implemented or released updater.

## Confirmed behavior

- Initial scope: Token Market desktop on macOS and Windows. CLI updates and
  managed Tool Upgrades retain their respective delivery paths.
- Initial architecture matrix: macOS Apple Silicon and Windows x64. Intel Mac
  and Windows ARM support are deferred pending their builds and device acceptance.
- Windows in-app installation initially supports current-user NSIS (.exe)
  installations only, as subsequently confirmed by the user. Keep MSI available
  for manual installation/recovery without mixing installer ownership; MSI
  auto-update requires separate lifecycle verification.
- Check for updates automatically at application startup and provide a manual
  check in Settings.
- Show the available version and release notes. Download and install only after
  the user confirms; allow the user to defer the update.
- No mandatory updates or unexpected restarts during user operations in the
  initial scope.
- Use a fixed HTTPS update address under a project-controlled domain, a static
  version manifest and domestically accessible object storage/CDN. GitHub remains
  the build and release-archive channel; no dedicated update backend is required
  initially. Hosting and domain configuration still need to be provisioned.
- Offer only stable releases to production clients. Distribute test builds by
  separate links, without an in-app release-channel switch in the initial scope.
- After confirmation, download and verify the package. Present an explicit
  "Install and restart" action before installation.
- Wait for tool installation/upgrade and connection-configuration mutations to
  finish before allowing client update installation.
- Allow update checks, downloads and installation without a Client Session,
  independently of workspace balance.
- "Later" suppresses unsolicited reminders for the rest of the current process;
  the next application startup may remind again. Do not periodically prompt
  during the same run.
- "Ignore this version" persistently suppresses automatic reminders for that
  exact version. Manual checks still show it, and a later stable version may
  prompt again.
- An automatic check failure must not interrupt startup. A manual check failure
  shows the reason and a retry action; it must not be reported as "up to date".

## Failure recovery and first installation

- A failed check, download or signature verification leaves the installed
  version in use. Offer retry without installing unverified bytes.
- Initial recovery is retry or manual installer-based recovery. If installation
  fails or the new application cannot start, provide a separately accessible
  official installer download and recovery instructions, preserving user data.
  Do not depend on a broken application opening its own recovery interface.
- Automatic rollback is outside the initial scope. Neither native installer
  recovery nor manual reinstallation is a guarantee that an arbitrary older
  version can read data written by a newer release.
- Existing versions without an updater require one manual installation of an
  updater-enabled release. Subsequent compatible releases can update in-app.
- Tool Connections, credentials, configuration and restoration records are not
  reset as part of a Client Update or manual recovery.

The distribution trade-off is recorded in
[ADR 0007](adr/0007-static-desktop-update-distribution.md).

Client Update and Tool Upgrade are distinct terms in the [glossary](../GLOSSARY.md).
Existing tool behavior is described in [desktop tool updates](desktop-tool-updates.md).

## Implementation constraints

- Preserve shared login, credentials, connection records, restoration backups and
  tool configuration, consistent with [ADR 0004](adr/0004-shared-client-key-lifecycle.md).
  Shared data changes must remain compatible with an independently updated CLI.
- Installation coordination must include CLI processes. Current login,
  connection and per-tool installation locks are separate and operation-scoped;
  a desktop-only busy flag is insufficient.
- Package the final platform-signed application before generating the updater
  archive and signature. The current macOS packaging step re-signs a copy after
  the Tauri build, so an earlier archive would not contain the final application.
- Updater signatures and platform code signing serve separate purposes. Preserve
  the existing signing identity unless a separately validated credential migration
  is provided; update signing does not establish Developer ID or notarization.
- Installer launch and completed installation are separate outcomes. Do not
  promise automatic recovery from later startup or data failures based on native
  installer behavior.

These constraints come from the shared storage/locking implementation, the
[macOS packaging flow](../scripts/macos-local-signing.py) and the
[Tauri updater documentation](https://v2.tauri.app/plugin/updater/).

## Manual check implementation

Settings now has a separate **客户端更新** section, available without a Client
Session or workspace balance. It reads the current native application version
and lets the user manually check a stable static manifest. A successful check
shows the newer supported version and plain-text release notes, or reports that
no newer supported stable version is available. Failures show a reason and an
explicit retry action. Discovery does not download, install or restart anything.

Build the desktop with `TOKEN_MARKET_UPDATER_ENDPOINT` set to the fixed HTTPS
static-manifest address and `TOKEN_MARKET_UPDATER_PUBLIC_KEY` set to the contents
of the Tauri signer's public key file. These are compile-time settings; the
renderer cannot replace either value. Both may remain empty until deployment
is provisioned, in which case manual checking reports the missing configuration.
The configured manifest and artifact URLs must use HTTPS, including redirects
in production. Only compiled native tests may opt into a loopback HTTP fixture.

The manifest uses Tauri's static format: `version`, optional `notes` and
`pub_date`, and `platforms.darwin-aarch64` / `platforms.windows-x86_64` entries
containing `url` and the `.sig` file contents in `signature`. Only versions newer
than the running application with no prerelease suffix are offered. Dynamic
single-package manifests, missing matching packages and empty signatures are
rejected. Downloaded bytes still require Tauri verification before installation;
a check result does not mean those bytes have been verified.

Isolated native integration tests use signed synthetic fixtures and the real
Tauri updater. Playwright tests exercise the existing UI-to-native boundary.
Run them with `cargo test --locked --manifest-path src-tauri/Cargo.toml` and
`node --test tests/desktop-ui.test.cjs` (Playwright and a browser are required).
These tests do not establish configured production hosting or device upgrades.

## Implementation and release work remaining

- Complete startup checks, persistent
  ignored-version preferences and the confirmed install/restart interaction.
- Add cross-process installation coordination without changing the independent
  CLI update path or interrupting running mutations.
- Provision the concrete HTTPS domain and storage/CDN; configure updater public
  keys and protect the corresponding release signing key outside the repository.
- Generate updater packages and signatures from the final platform-signed
  application. Publish all artifacts successfully before making a new stable
  manifest visible, so clients never discover incomplete release assets.
- Keep separately accessible official installers and recovery instructions.
- Validate macOS Apple Silicon and Windows x64 upgrades from an older installed
  version on actual devices. Cover startup/manual checks, logged-out use,
  "Later"/ignored-version behavior, active desktop and CLI mutations, failed
  network and signature checks, installer failure handling and successful
  post-update startup with shared credentials and connections retained.
- Keep build, updater-signature validation, platform signing/notarization,
  deployment and device acceptance as separate evidence. None is established by
  this design document.

Older deployed desktop versions contain no built-in updater and require one
manual installation of an updater-enabled release. The existing release workflow
publishes installers; source implementation alone does not establish that deployed
clients can already update themselves.

## Release distribution tooling

See [desktop update release distribution](desktop-update-distribution.md) for
final-signature preparation, isolated OSS candidate publication, independent
recovery installers and the real native-client published-fixture check. Hosting
configuration may remain empty. Production promotion and actual platform
installed-client acceptance remain separate #38 work.
