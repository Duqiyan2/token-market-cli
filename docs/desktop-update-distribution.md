# Desktop update release distribution

Alibaba Cloud OSS is the selected origin. Domain, bucket, identity and release
keys may remain unconfigured. The commands below prepare release inputs and
publish isolated candidates; a separately gated [promotion command](client-update-promotion.md)
validates operator acceptance before stable publication. Real installed-client
acceptance and actual promotion remain open under #38. No resource provisioning, production manifest
publication or real-device upgrade is established by this implementation.
GitHub workflows still build installers and retain release archives.

## Toolchain and build inputs

The build and signer use **Tauri CLI 2.12.4**, whose released source pins
**tauri-bundler 2.10.4**. This pins the NSIS template's `/UPDATE`, `/P`, `/R` and
currentUser behavior rather than relying on whichever CLI tauri-action installs.
The command rejects a different signer version. The actual update verifier is
**tauri-plugin-updater 2.14.1**, pinned exactly in Cargo and locked. Reassess the
Windows handoff before changing any of these pins. The first supported Windows
automatic path is currentUser NSIS (`windows-x86_64-nsis`); MSI remains a manual
installer and is never an in-place automatic update target.

Build clients with `TOKEN_MARKET_UPDATER_ENDPOINT` (the project-owned fixed HTTPS
manifest URL) and `TOKEN_MARKET_UPDATER_PUBLIC_KEY` (contents of the public key).
These values may be empty until provisioned. Keep `requireSignedVersion: true`.
Only that public key belongs in the application; public/private key rotation is
a separate migration. Updater signatures do not establish OS code-signing or
notarization. Preserve the shared credential signing identity.

Build the release-host verifier before using prepare/publish:

```sh
cargo build --locked --manifest-path src-tauri/Cargo.toml --example verify-update-artifacts
```

Use `src-tauri/target/debug/examples/verify-update-artifacts` (append `.exe` on
Windows) as `--verifier`. It creates a private loopback metadata service and
calls the official updater's `check` and `Update::verify` with signed-version
validation. It never installs. MockRuntime replaces the windowing runtime only;
the verifier and signature/version enforcement are the official updater. Build
it on a desktop build host with Tauri's native prerequisites, not a minimal
headless Linux uploader. The product has no new insecure HTTP setting.

## Prepare final bytes on each platform

Private signing input is **only** `TAURI_SIGNING_PRIVATE_KEY` and optionally
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in the CI process environment. Never put it
in a command argument, inventory, artifact, repository, log or chat. Path-based
private key fallback is rejected. Use the protected CI secret store; prepare
captures signer output and suppresses it even on failure.

On macOS, first run the existing production `macos-local-signing.py package`
step. Pass its **output** `.app`, DMG and `signing-manifest.json`, never the
pre-signing Tauri app/archive. For example, with paths selected by your CI job:

```sh
python3 scripts/desktop-update-release.py prepare \
  --target darwin-aarch64 --version "$RELEASE_VERSION" \
  --app "$FINAL_SIGNED_APP" --manual-installer "$FINAL_DMG" \
  --platform-evidence "$MACOS_SIGNING_MANIFEST" \
  --public-key "$UPDATER_PUBLIC_KEY_FILE" --signer "$TAURI_CLI" \
  --verifier "$UPDATER_VERIFIER" --output "$PREPARED_MACOS_DIRECTORY"
```

Prepare copies the finalized app, verifies its strict code signature,
certificate SHA-1 against the production signing manifest, bundle identifier,
version and arm64 architecture, then creates `.app.tar.gz` from that copy.
It checks the DMG digest against the same signing manifest and verifies DMG
integrity. **Only then** does it sign the exact archive using
`signer sign --app-version VERSION`, and run real updater verification.
Notarization remains separately recorded, never inferred from this check.

On Windows, sign the final application executable and installers with the
intended Authenticode identity before prepare. Supply an evidence JSON containing
`certificateSha1` (the public certificate thumbprint). Prepare requires
`Get-AuthenticodeSignature` status `Valid` and that exact identity for both the
application and installer, matching application ProductVersion and PE x64.
Build NSIS with the confirmed currentUser policy using the pinned bundler. Prepare
also requires explicit `bundle.windows.nsis.installMode: currentUser` in
`src-tauri/tauri.conf.json` (or the actual `--tauri-config` used for this build),
and records that policy and the pinned bundler in its receipt.

```sh
python3 scripts/desktop-update-release.py prepare \
  --target windows-x86_64-nsis --version "$RELEASE_VERSION" \
  --app "$FINAL_SIGNED_EXE" --installer "$FINAL_SIGNED_NSIS" \
  --manual-installer "$FINAL_SIGNED_MSI" \
  --platform-evidence "$WINDOWS_SIGNING_EVIDENCE" \
  --public-key "$UPDATER_PUBLIC_KEY_FILE" --signer "$TAURI_CLI" \
  --verifier "$UPDATER_VERIFIER" --output "$PREPARED_WINDOWS_DIRECTORY"
```

`windows-x86_64-msi` can also prepare a signed MSI receipt for manual delivery;
publication never adds that target to the updater manifest. Prepare outputs an
artifact, `.sig` and public `prepared.json` receipt in a new directory. Failure
leaves the requested output absent; existing output is never overwritten. Any
subsequent artifact modification invalidates the receipt/digest/signature.

## Assemble and plan a candidate

Assemble a release inventory JSON from the two prepared receipts. Paths are
relative to the inventory (absolute paths are also accepted). Required shape:

```json
{
  "version": "1.1.0",
  "source_commit": "<full lowercase commit SHA>",
  "notes": "Release notes shown to users",
  "public_key": "<public key file contents>",
  "targets": {
    "darwin-aarch64": {
      "artifact": "mac/TokenMarket.app.tar.gz",
      "signature": "<mac prepared receipt signature>",
      "platform_signing": {"verified": true, "kind": "codesign", "artifact_sha256": "<receipt digest>"}
    },
    "windows-x86_64-nsis": {
      "artifact": "windows/TokenMarket-1.1.0.exe",
      "signature": "<Windows prepared receipt signature>",
      "platform_signing": {"verified": true, "kind": "authenticode", "artifact_sha256": "<receipt digest>", "bundler_version": "2.10.4", "nsis_install_mode": "currentUser"}
    }
  },
  "installers": [
    {"path": "mac/TokenMarket.dmg", "platform": "darwin-aarch64", "platform_signing": {"verified": true, "kind": "codesign", "artifact_sha256": "<manual receipt digest>"}},
    {"path": "windows/TokenMarket.msi", "platform": "windows-x86_64", "platform_signing": {"verified": true, "kind": "authenticode", "artifact_sha256": "<manual receipt digest>"}}
  ]
}
```

Copy the complete `platform_signing` records from receipts, including public
identity evidence; the abbreviated JSON above illustrates the required shape,
not evidence for a real release. Production receipts must come from the trusted
native CI signing jobs; arbitrary JSON assertions are not a substitute for
running native platform verification. Never assemble inventory from unsigned
build-verification records. Notes, both updater targets, signatures and both
platforms' manual installers are required before any publication write.

```sh
python3 scripts/desktop-update-release.py plan --inventory "$INVENTORY" \
  --manifest-key test/release-candidate/stable.json
```

Plan is read-only, deterministic, requires no credentials or network and prints
missing deployment **field names** when unset. It never invents a working URL.
It reports exact source digests, immutable object keys, public URLs when supplied
and cache/content headers. It does not claim signatures were verified; prepare
and publish perform that verification. Prerelease versions are rejected.

## Provisioning inputs (all currently unset)

| Input | Purpose |
| --- | --- |
| `OSS_REGION` | Native ossutil region |
| `OSS_ENDPOINT` | Native ossutil upload endpoint; separate from public download domain |
| `TOKEN_MARKET_UPDATE_OSS_BUCKET` | Intended bucket, restricted publishing permissions |
| `TOKEN_MARKET_UPDATE_OSS_PREFIX` | Nonempty isolated object prefix |
| `TOKEN_MARKET_UPDATE_PUBLIC_BASE_URL` | Anonymous public HTTPS base mapping **to that object prefix**; no query, userinfo or fragment |
| `TOKEN_MARKET_UPDATE_OSS_ADDRESSING_STYLE` | Optional `cname` when required by the account's upload endpoint policy |
| `OSS_ACCESS_KEY_ID`, `OSS_ACCESS_KEY_SECRET`, `OSS_SESSION_TOKEN` | Short-lived CI STS credentials, env only |
| `OSSUTIL_CONFIG_FILE`, `OSSUTIL_PROFILE` | Explicit provider config/profile alternative, including supported OIDC mode |
| `TOKEN_MARKET_UPDATER_ENDPOINT`, `TOKEN_MARKET_UPDATER_PUBLIC_KEY` | Compile-time client discovery URL and trust key |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Protected CI-only updater signing input |

Prefer the provider-maintained GitHub OIDC action exchanging a restricted role
for short-lived STS credentials; map its outputs explicitly to the three native
`OSS_*` credential variables. Do not assume `ALIBABA_CLOUD_*` SDK variables
configure ossutil. An actual trust policy, protected environment, least privilege
role, custom domain/DNS/CNAME, matching TLS certificate, public-read route and
CDN cache policy must be supplied and validated before deployment. None is
created by this script. Do not enable public writes. The uploader inherits bucket
ACLs; it never changes them. A future workflow must pin a reviewed identity-action
commit and restrict its repository/ref/environment trust before granting
`id-token: write`.

Use **ossutil 2.4.1**, verify its official per-platform download checksum and make
it available as `--ossutil`. Publish checks its version and 2.x `cp --help` before
writing. It uses single-file `cp`, `--cache-control`, `--content-type`,
`--metadata sha256=...`, and `--ignore-existing` for immutable objects; it does
not use legacy 1.x `--meta Header:value` syntax or recursive sync.

```sh
python3 scripts/desktop-update-release.py publish --inventory "$INVENTORY" \
  --manifest-key test/release-candidate/stable.json \
  --verifier "$UPDATER_VERIFIER" --ossutil "$OSSUTIL"
```

This command is provided for a future configured **isolated** destination. Do
not execute it against live infrastructure until authorized. It cannot publish
`stable.json` or another production discovery route: only `test/.../stable.json`
is enabled on `publish`. Use the separately gated [promotion command](client-update-promotion.md)
only with complete build provenance and actual installed-client acceptance; the
candidate publisher is not that gate.

Before any write, publish validates the complete inventory, snapshots every file
and verifies exact artifact bytes and signed version through the real updater.
It uploads immutable `releases/VERSION/SHA256/filename` objects and a versioned
recovery HTML page, then streams a public anonymous GET for each and validates
SHA-256, length, content type, no content encoding and cache policy. Missing,
truncated, transformed or stale data aborts publication. Existing immutable
objects are retained; conflicting bytes are rejected by public readback.
Only after **all** readbacks pass does it perform the single last manifest PUT,
with `no-cache, no-store, max-age=0, must-revalidate`, followed by manifest GET.
An error in the final readback may mean that final PUT succeeded; inspect the
public manifest before retrying. All failures before it leave old discovery
untouched. Artifacts use `public, max-age=31536000, immutable`; `.tar.gz` is
`application/gzip` without HTTP `Content-Encoding: gzip`.

Serialize **all** publishers to the same destination in one protected CI
concurrency group (`cancel-in-progress: false`). The fixture store has an
exclusive publisher lock; an uploader-local lock cannot serialize distinct CI
runners. OSS atomic/strongly consistent PUT is the final discovery switch,
not a multi-object transaction or a release-job lock. Configure CDN manifest TTL
zero and respect origin headers; origin settings alone cannot override a CDN's
own cache policy. Retain previous immutable artifacts for manual recovery.

## Independent recovery and bootstrap

Publish reports a versioned public `recovery_url`, alongside `manifest_url`.
Link it from the official externally reachable download/support site so users
can open it without starting Token Market. Configure that site's route to the
reported URL; no live address is invented here. The generated HTML links both
platforms' verified manual installers and explains one-time installation for
clients lacking an updater, retry/manual recovery, preserving shared data and
credentials, no automatic rollback and no arbitrary downgrade compatibility.
A completed candidate is not public production recovery until hosting and that
external navigation are actually configured and verified in #38.

If recovery is necessary, quit the app, install the same or a supported newer
official release over the existing installation, and retain the shared client
data directory, system credentials, Tool Connections and restoration backups.
Do not recommend deleting data or arbitrary downgrades. Windows MSI remains
manual recovery; it is not advertised as an automatic NSIS migration path.

## Isolated verification

`--fixture --local-store DIRECTORY` publishes only into a local directory and
allows an explicitly configured loopback HTTP public base. Fixture signing
receipts can never reach the OSS adapter. Tests use synthetic bytes and a
committed public key/signatures; a real-signer check generates its disposable
private key only inside a temporary fixture directory. No real release secret,
account, resource or stable production object is accessed.

```sh
cargo build --locked --manifest-path src-tauri/Cargo.toml --example verify-update-artifacts
TOKEN_MARKET_TEST_TAURI_SIGNER="$TAURI_CLI" python3 -m unittest discover -s tests -p test_update_distribution.py
cargo test --locked --manifest-path src-tauri/Cargo.toml release_distribution_client -- --ignored --nocapture
```

The last test starts the actual publisher fixture command, then uses the native
`ClientUpdater` for both target checks, downloads and real signature/version
verification. No installation or shared credentials are accessed. It is marked
ignored in the ordinary unit suite because it requires the separately built
release verifier. The dedicated manual workflow runs it explicitly. These
checks establish package/publication behavior; they do not establish genuine
release OS signing, live OSS/TLS/CDN readability, macOS installed upgrade,
Windows device acceptance or production promotion.

Primary references: [pinned CLI/bundler source](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.4/Cargo.toml),
[pinned NSIS template](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.4/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi),
[Tauri signer](https://v2.tauri.app/reference/cli/#signer-sign),
[OSS 2.x configuration and official binaries/checksums](https://www.alibabacloud.com/help/en/oss/developer-reference/ossutil-overview/),
[OSS 2.x upload flags](https://www.alibabacloud.com/help/en/oss/developer-reference/cp-upload-file),
[provider OIDC action](https://github.com/aliyun/configure-aliyun-credentials-action),
[OSS atomicity/consistency](https://www.alibabacloud.com/help/en/oss/terms-2).
