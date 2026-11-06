# Windows current-user NSIS update acceptance

Automatic installation is restricted to Windows x64 NSIS `currentUser` builds.
MSI remains a manual installation/recovery channel. Old NSIS builds without the
installation policy marker also require one manual installation of the new NSIS
build before they become eligible. No installer ownership is migrated by the
updater.

## Automated native fixture

On a disposable Windows x64 runner with Rust installed:

```powershell
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --test client_update_guard --test client_update_guardian --locked
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked
cargo test --manifest-path src-tauri/Cargo.toml --features windows-update-fixtures --test windows_guardian --locked -- --nocapture
```

The Windows fixture copies the actual built desktop executable outside its
synthetic installation directory and dispatches the guardian before Tauri or
session setup. A separately compiled native fixture installer stays alive until
the test releases it. The parent exits as a real process; legacy lock probes
must still refuse work throughout installation. The fixture checks an exact
process handle, stage write denial, the native unquoted final `/D=` command line
with spaces and Chinese names, cancellation/retry and old-version reconciliation.
The native fixture also checks known launch failure/nonzero termination,
signature reverification of actual staging bytes, the protected staging DACL
and refusal of raw, unmarked, mismatched, MSI and system-owned installations.
It accesses only a temporary synthetic home and fresh scoped test registry
subtrees and never launches the normal desktop or credential setup.

These tests are process/OS-lock evidence, not an installed NSIS upgrade, physical
device, production endpoint or credential-retention acceptance. The
[hosted Windows validation](https://github.com/Duqiyan2/token-market-cli/actions/runs/41964120322/job/127037038487)
passed for candidate `db7eadd` (Actions PR merge `cbaf5f3`): all **8 Windows
guardian fixtures** passed, along with **20 desktop library tests** and the
shared core process tests. The production factory now registers only on x86_64
after the normal current-user NSIS bundle/registry ownership checks succeed;
fixture features do not bypass production registration checks. A whole-package
NSIS build and installed-client acceptance remain separate checks.

## Installed-client check still required

Use a disposable Windows x64 account/device and two distinct signed stable
versions built with Tauri CLI 2.12.4 / bundler 2.10.4. Record both commit IDs,
manifest/artifact signatures, actual endpoint and installed path. Use the
project-controlled test endpoint; do not publish a stable manifest for this check.

1. Manually install the initial NSIS build as the current user. Confirm the
   NSIS bundle stamp and HKCU policy/directory marker. Check that MSI, portable,
   system-level and unmarked installations keep automatic installation disabled.
2. Establish synthetic login, encrypted state and Tool Connections. Record
   non-secret checksums/observable behavior for workspace ownership, model
   selection, tool configuration and restoration records. Keep the independent
   CLI on its original version and update channel.
3. Run an independent CLI connection/tool/login mutation. In the desktop,
   download and verify the newer package and explicitly choose Install and
   restart. The operation must refuse as busy without cancelling CLI work.
   Let that work finish, then retry explicitly.
4. Observe the copied guardian running outside the installation directory before
   the desktop exits. While the installer runs, a separately installed legacy
   CLI must refuse each protected operation using its existing lock. No test
   should kill a live installer or guardian.
5. After known installer termination, confirm the exact installed executable
   relaunches and its actual running compiled version equals the target. Only
   that startup may mark the journal successful. A zero installer exit, launch
   request, registry version or old application restart must not claim success.
6. Recheck the synthetic state and the unchanged independent CLI. Confirm the
   normal NSIS update does not uninstall/delete shared application data.
7. Repeat in a fresh fixture with precommit cancellation, installer cancellation
   or nonzero exit, unavailable launch, blocked/failed relaunch and old-version
   restart. Record actionable manual recovery and explicit retry behavior.
   Never clear an unresolved record based on elapsed time.

No automatic rollback is promised. An unreadable or unresolved journal stays in
recovery until an installer/version fact is established. An externally killed
guardian loses Windows process-owned legacy locks; exclusion against arbitrary
guardian termination is outside this contract. Covered login scopes are the
current origin plus already-existing scopes, not arbitrary new origin namespaces.
