# Desktop update release distribution

The release policy is local native build/signing and local Alibaba Cloud OSS
publication. Follow the [Chinese operator guide](client-update-local-release.zh-CN.md)
for the first package and later releases, and the [promotion contract](client-update-promotion.md)
for evidence fields. Resources are intentionally unconfigured. No production
upload, actual device acceptance or public updater release is established here;
#38 and parent #30 remain open.

## Public command contract

The entry point is `python3 scripts/desktop-update-release.py`. The local JSON
[example](client-update-local.example.json) has no GitHub fields. Copy it outside
all Git checkouts and fill it there. It is parsed as JSON, never evaluated as shell.
Paths to config, keys, profiles, output, evidence and state must be absolute and
outside Git; symlink-resolved paths into any checkout are rejected. Native build
source is a clean checkout. Do not source the legacy environment example for a
real release.

| Command | Behavior |
| --- | --- |
| `local-build --config FILE --target TARGET` | Invoke the default native build/sign recipe, verify final platform signing, prepare/sign exact updater bytes, verify the official signed version, and atomically emit prepared files and schema2 local `build.json` |
| `local-inventory --config FILE --prepared MAC/prepared.json --prepared WINDOWS/prepared.json --notes-file FILE` | Collect both finalized outputs from the same commit/version/compiled routes/key into a verified inventory; signatures are copied automatically |
| `local-candidate --config FILE --inventory FILE` | Validate local build/prepared records and publish only the configured isolated `test/.../stable.json` |
| `local-validate --config FILE --inventory FILE --acceptance FILE` | Perform acceptance and updater validations on snapshots; no OSS writes, public GETs, credential reads or publisher locks |
| `local-publish --config FILE --inventory FILE --acceptance FILE` | Explicitly promote the accepted exact bytes, with the same validations and final public readbacks |

Targets remain `darwin-aarch64` and `windows-x86_64-nsis`. Both are required in an
inventory and in device acceptance. macOS uses the existing persistent identity
via `build-macos-clients.sh`; this command never initializes or rotates it. Its
current signing manifest honestly records `notarized: false`. Windows invokes
`build-windows-clients.ps1`, builds/signs x64 currentUser NSIS using Tauri's
structured `signCommand`, and retains the existing installer hooks/marker. The
signed NSIS bytes also supply the manual installer under a distinct filename;
MSI remains an optional manual/recovery format, never an automatic target.

A target build needs only its native resources, the shared compile configuration,
updater signing key and tools. It does not need actual resources for the other
host or an OSS credential profile. Native Cargo package version and Tauri JSON
version must both match the configured desktop version. CLI delivery/versioning
remains independent. Native children receive the explicit compile inputs and
external Cargo output directory; signer/provider output is suppressed.

## Tools and immutable bytes

Use Python3.9+, Tauri CLI **2.12.4** (bundler **2.10.4**), ossutil **2.4.1**,
and the real verifier linked against official `tauri-plugin-updater/2.14.1`:

```sh
CARGO_TARGET_DIR="$PRIVATE_DIR/verifier-target" cargo build --locked \
  --manifest-path src-tauri/Cargo.toml --example verify-update-artifacts
```

Build this verifier on the machine running each release command; it does not
launch installers. `tools.verifier` points to its native executable.
Native Windows signing additionally needs an installed certificate/private-key
provider, SignTool and an explicit RFC3161 timestamp URL (provider HTTP or HTTPS
is supported). See [Tauri signing](https://v2.tauri.app/distribute/sign/windows/)
and [structured signCommand](https://v2.tauri.app/reference/config/#customsigncommandconfig).

Prepared signatures cover the final archive/NSIS bytes, with `--app-version`;
wrong keys, signatures, modified bytes and signed-version mismatch fail before
publication. Native records bind final artifact hashes/sizes, signing identity,
prepared receipt hash, source/version and the exact compiled endpoint/recovery/key
fingerprint. The configuration hash covers the shared public compile policy,
not host-specific private file locations. These are trusted local operator
records, not independent cryptographic build or device attestation.

## OSS publication order and serialization

Set an explicit external ossutil profile (`oss.config_file`, `oss.profile`). The
local process clears ambient OSS/OSSUTIL/GitHub/updater/signer credential inputs
and supplies the selected configuration; credential values never appear in argv
or errors. The actual upload uses native ossutil2.4.1 `cp` with `--cache-control`,
`--content-type`, `--metadata sha256=...`, `--ignore-existing` for immutable objects
and `--force` for discovery aliases. See [ossutil configuration](https://www.alibabacloud.com/help/en/oss/developer-reference/ossutil-overview/).

Candidate and stable share a destination-specific nonblocking OS file lock on
one designated publishing account, independent of checkout/config/output
paths. Persistent lock files are in `/tmp/tokenmarket-publisher-UID/` on Unix and
`%LOCALAPPDATA%/Token Market/Release Publisher Locks/` on Windows. OS locks release
on process exit; do not delete lock files while a publisher is running. This is
not a distributed lock across computers/accounts: disable CI and other-machine
writers to the same bucket/prefix. Optional operator records may live in an
external directory; they do not configure the common lock namespace.

Each invocation snapshots and verifies the actual upload bytes. Under the lock,
production publication first reads current public stable: genuine404 permits
bootstrap; malformed, auth or network failures stop publication. An older version
cannot replace a newer one; a same-version publication requires identical manifest
bytes. All updater/manual packages and recovery HTML go to immutable
`releases/VERSION/SHA256/filename` paths. Every object is anonymously fetched with
exact SHA256, size, content type, no content transformation and cache policy checks.
The fixed recovery alias is PUT and read back **before** production stable; stable
PUT/readback is the final remote step. A final readback failure reports that
discovery may have advanced, not a rollback guarantee.

Artifact cache policy is `public, max-age=31536000, immutable`; fixed stable and
recovery aliases use `no-cache, no-store, max-age=0, must-revalidate`. File types:
`.app.tar.gz` application/gzip, installers application/octet-stream, recovery
text/html, manifest application/json. Configure domain/CDN/OSS so anonymous
requests preserve these headers and exact bytes. The public base maps the
configured OSS prefix; do not duplicate the prefix in both routing layers.

## Isolated checks

Low-level `plan`, `prepare`, fixture `publish` and fixture `promote` remain for
isolated integration tests. Real publishing through the old environment/CI
interface is disabled; `record-build` has been removed. Old schema1/CI records
cannot pass local validation. Local commands' `--fixture` classification may use
synthetic native adapters and loopback HTTP, but candidate/promotion requires
`--local-store`; that mode cannot execute ossutil even with populated secrets.
No fixtures fill real actual-device acceptance slots.

```sh
TOKEN_MARKET_TEST_TAURI_SIGNER=/absolute/path/to/tauri \
  python3 -m unittest discover -s tests -p 'test_update_*.py'
# Existing explicit installed-native fixture seam, using the published local store:
cargo test --locked --manifest-path src-tauri/Cargo.toml \
  release_distribution_client -- --ignored --nocapture
```

Tests use disposable keys, synthetic apps/installers and a local HTTP store. They
do not prove Windows/macOS native build success, signing/notarization provision,
actual installed upgrades, live OSS/TLS/CDN readiness or stable promotion.
