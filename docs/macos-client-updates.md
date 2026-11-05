# macOS Client Update installation

The initial native installer supports Apple Silicon application bundles installed
directly in `/Applications` or the user's `~/Applications`. Mounted DMGs,
translocated/download locations, development executables and symlinked bundle
paths do not enable installation. Move the official app to Applications, or use
the official manual installer. Intel/universal packages and archives containing
links are outside this initial automatic-install scope.

The desktop offers installation only after a verified native download and an
explicit **Install and restart** confirmation. `MacInstaller::prepare` acquires
the complete core legacy lock set before validating the app archive and returning
its owned handoff. A conflict preserves active desktop/CLI work and verified
bytes for a deliberate retry. The handoff keeps all locks through synchronous
`tauri_plugin_updater::Update::install`, durable outcome recording and the call
to `AppHandle::restart`. There is no unguarded direct-install IPC command.

Before replacement, the native executor checks a single safe app archive,
application identifier/name/executable, installed and target versions, arm64
architecture, strict code signatures and **exact equality of the existing
certificate-pinned shared CLI/desktop designated requirement**. An accidental
certificate or credential identity change is refused. Ad-hoc signatures do not
qualify. The update does not modify shared storage, perform migration, reconnect
tools, rotate keys or change tool configuration. The independent CLI keeps its
existing storage paths and identity.

The journal records original/target version and the actual native executable
path. A successful synchronous install records `AwaitingStartup` before an
explicit restart; neither launch nor the old process exiting means success.
Early native setup reconciles the journal before checking distribution
configuration, including when the host URL/key is blank. Only the target
compiled version running from the recorded executable path acknowledges
`Succeeded`. Old/wrong executable startup remains pending with recovery guidance.
After
reacquiring the whole guard, macOS can prove its earlier synchronous installer
is inactive: an interrupted `InstallerRunning` advances to `AwaitingStartup`,
then the same strict actual-version/path acknowledgment applies. Interrupted
`Preparing`, `Guarded` or `Committed` phases never called install, so they become
known failures under that guard and allow a fresh explicit retry. These facts
do not apply to an independent Windows installer process. Known failed
installation keeps verified bytes for retry;
an uncertain completed replacement returns a typed pending handoff with an
actionable recovery warning and cannot silently become a new attempt. A failure
to persist the final startup-wait phase does not restart or enable retry; actual
matching target startup can safely reconcile it under the macOS guard.

Set `TOKEN_MARKET_UPDATER_RECOVERY_URL` at build time to the publisher-controlled
HTTPS recovery page. Native code validates and opens it; the renderer receives
only availability and cannot supply a URL. Without configuration, Settings keeps
the recovery action disabled and tells the user to contact the publisher. Do not
invent a production recovery domain. Manual recovery must preserve client data;
automatic rollback and credential/signing-identity migration are excluded.

## Reproducible isolated verification

On macOS Apple Silicon, run:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Native fixtures use a real separately signed arm64 app archive, actual updater
verification and synchronous application replacement under temporary storage.
They substitute only the high-level restart, execute the new inert version,
check guard ownership at restart, retain synthetic shared data, and reject old
startup paths, changed signing identity and invalid archives. A system-boundary
filesystem fault proves actual official install failure retains verified retry.
The tests require loopback sockets and process execution; they never install or
launch a real user's app. See the [fixture description](../src-tauri/tests/fixtures/client-update/README.md).

For actual installed-client acceptance, use two updater-enabled versions signed
with the existing release identity and a provisioned test HTTPS manifest. On a
dedicated Apple Silicon acceptance device, record the starting installed app,
synthetic Client Session/Tool Connections and independent CLI compatibility;
exercise desktop/CLI busy refusal, then explicit download/installation/restart;
confirm the actual target compiled version and journal success, the same
credential identity/configuration/ownership/restoration records, and the external
recovery route. Keep that device evidence separate from these fixtures and the
existing signed-process synthetic credential probes. Production hosting,
Developer ID/notarization and actual production-domain device acceptance are
not established by the automated checks here.
