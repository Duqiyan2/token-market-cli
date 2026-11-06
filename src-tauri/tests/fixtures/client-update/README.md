# Synthetic updater fixture

`package.txt` contains inert test bytes, signed for version `1.1.0` with the
official Tauri CLI signer. `public-key.txt` and `package.txt.sig` are public test
material. The private test key is not committed. This fixture is not an
application archive and must never be installed.

Manifest checks use the real Tauri updater against a test-only loopback service.
They assert that discovery never requests the artifact. Checking metadata does
not establish package verification or a working platform installation.

Confirmed-download tests use the real updater to verify these bytes with signed
version enforcement. `wrong-key.sig` signs the same bytes and version with a
separate synthetic key and must fail against `public-key.txt`. The matching wrong
private key is also outside the repository. Tests also cover altered bytes,
malformed/missing signatures, a mismatched advertised version, connection loss,
HTTP failure, retry, stale consent and checks during downloading/ready states.
No fixture operation calls install or restart. Production requests and redirects
remain HTTPS-only; loopback HTTP is confined to native test configuration.

## macOS signed application fixtures

`macos/installed.tar.gz` and `macos/update.tar.gz` contain inert arm64 Mach-O
executables which print compiled versions `1.0.0` and `1.1.0`. Their matching
certificate-pinned designated requirements use a **disposable synthetic** signing
identity. `changed-identity.tar.gz` carries a separate synthetic certificate and
must be refused even though its updater signature verifies. These identities
are unrelated to the release identity and provide no Developer ID/notarization
or real credential-store acceptance evidence. Only public archives, signatures
and updater public key are committed; no signing private key is included.

Native tests extract only into `tempfile` directories, download with the actual
Tauri updater, call its actual synchronous macOS installer against a disposable
application path, and substitute only the high-level restart action. They never
install, launch or reset the user's application/home. The installed inert binary
is run to observe its actual compiled target version. Startup reconciliation is
also tested independently with target/old version and matching/wrong native
executable paths. Shared synthetic data must remain byte-for-byte intact.

To regenerate public fixtures on a macOS Apple Silicon machine with Node/Tauri
CLI dependencies installed, explicitly run:

```sh
NODE_PATH=/path/to/node_modules python3 src-tauri/tests/fixtures/client-update/macos-fixture.py /private/tmp/tokenmarket-macos-updater-fixtures
```

Use a new empty temporary output directory. The generator creates private
synthetic signing keychains, removes them from the user's search list using the
existing signing helper, and deletes those keychains on exit. It does not add
trust or inspect release identities/credentials. It deletes its updater private
key after signing. Copy only the six public archive/signature/key files from
that output directory into `macos/`. Normal native tests use the committed public
fixtures and do not create/unlock signing keychains. The separate existing
signed-process synthetic credential smoke remains a distinct validation step.
