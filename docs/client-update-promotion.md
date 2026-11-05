# Accepted desktop release promotion

The promotion command validates a complete operator acceptance record and the
exact accepted bytes before publishing. The implementation and isolated fixtures
are available; actual macOS Apple Silicon and Windows x64 installed-client
acceptance, OSS/TLS/CDN configuration, external official navigation and production
promotion remain pending under #38. No Windows acceptance device is available.
Do not close #38 or its parent because these command tests pass.

`publish` continues to accept only `test/.../stable.json`. `promote` is a separate
boundary with no `--allow-production` switch. Configuration is deliberately empty
in [client-update-release.env.example](client-update-release.env.example).
There is no credentialed production workflow. This document is the deployment
contract for one to be reviewed and provisioned later; no resources or publishing
credentials are created by the script.

## Fixed build configuration and candidate acceptance

Choose the final production manifest object key and fixed recovery HTML alias
**before** building either platform. Compile both targets with the exact intended
`TOKEN_MARKET_UPDATER_ENDPOINT`, `TOKEN_MARKET_UPDATER_RECOVERY_URL`, and public
`TOKEN_MARKET_UPDATER_PUBLIC_KEY`. The production manifest URL is the public base
plus the configured manifest key; the fixed recovery URL is the same base plus
its configured recovery key. Both must be HTTPS. Never embed an artifact digest
in the fixed recovery route: recovery HTML itself links final installer digests,
which would otherwise create a build-time dependency cycle.

Publish the final candidate bytes through the isolated `publish` command. Obtain
an older updater-enabled **installed acceptance client** compiled to discover
that isolated candidate URL on the same configured HTTPS service. Record its
source/configuration and installation in the operator evidence. Its discovery
route is distinct from the candidate's compiled production endpoint. Do not
change production stable early, rewrite a candidate binary's endpoint, or rebuild
the accepted binary for promotion. A client without an updater must separately
exercise manual bootstrap and a subsequent in-app update.

Each inventory target gains two paths, relative to the inventory directory:

```json
{
  "prepared_receipt": "darwin-aarch64/prepared.json",
  "build_provenance": "darwin-aarch64/build-provenance.json"
}
```

Use the matching Windows paths for `windows-x86_64-nsis`. Inventory installer
paths may include those directories; prepared receipts retain basenames. Keep
finalized files immutable after acceptance. Production package signing evidence
must retain the actual `certificate_sha1`; macOS must explicitly state
`notarized: true` or `false`. Passing updater verification never implies
notarization. Windows automatic packages remain currentUser NSIS only; MSI is
manual installation/recovery.

## Evidence records

Start from [client-update-acceptance.template.json](client-update-acceptance.template.json)
and [client-update-build-provenance.template.json](client-update-build-provenance.template.json).
They intentionally contain empty values, `pending`, and `actual_device: false`
and cannot pass promotion. Schema version 1 is exact: omitted and extra fields,
empty references, truthy strings and generic `passed: true` are rejected. Both
required target records and every named check are mandatory.

The release binding contains full source SHA, stable version, SHA-256 of the
candidate manifest bytes, and the digest/size/role/platform of **every** updater
and manual installer. Manifest hashing uses the publisher's UTF-8 JSON with
2-space indentation, insertion order `version`, `notes`, `platforms`, and one
trailing newline. Obtain the exact bytes from the candidate publisher; do not
hash arbitrary reformatting. The candidate and final manifests contain the same
artifact URLs/signatures. Both macOS and Windows records bind the compiled
production endpoint, fixed recovery URL and SHA-256 of the trimmed updater public
key string encoded as UTF-8. This is an updater key fingerprint, distinct from
Agent API Key fingerprints and native certificate SHA-1 identities.

Each separate build provenance file must be retained as output of a reviewed,
trusted native build run after final signing and preparation. It records the
exact repository, workflow path, Actions run URL and attempt, source SHA, target,
version, all target artifact digests/sizes, prepared receipt digest, native
identity and compiled configuration used by that build. Configure
`TOKEN_MARKET_UPDATE_TRUSTED_BUILD_REPOSITORY` and the JSON mapping
`TOKEN_MARKET_UPDATE_TRUSTED_BUILD_WORKFLOWS` with exact approved native workflow
paths for both targets. Existing `prepared.json` receipts **do not** establish
source SHA or compiled routes/key; a receipt alone is rejected. Historical CI
success without this provenance does not satisfy the gate.

The repository provides a public native-job emission command; provenance is not
intended to be written by hand. After the platform's actual build, final native
signing and existing `prepare` step, run this in that **same clean native CI
checkout and unchanged job environment**:

```sh
# PREPARED_RECEIPT is the prepared.json produced by this job's prepare command.
python3 scripts/desktop-update-release.py record-build \
  --prepared-receipt "$PREPARED_RECEIPT" \
  --output "$RUNNER_TEMP/build-provenance.json" \
  --verifier "$UPDATER_VERIFIER"
```

Keep `TOKEN_MARKET_UPDATER_ENDPOINT`, `TOKEN_MARKET_UPDATER_RECOVERY_URL` and
`TOKEN_MARKET_UPDATER_PUBLIC_KEY` at the **job** level for both the actual Tauri
build and this command, sourced from the protected production configuration. Do
not change them between build, native signing, `prepare` and `record-build`.
The command reads actual `git rev-parse HEAD`, requires it to match `GITHUB_SHA`,
requires a clean tracked checkout and matching checked-out desktop version,
validates the configured native main workflow against `GITHUB_WORKFLOW_REF`,
records Actions run/attempt, computes artifact and prepared-receipt digests, checks
native identity metadata and runs the real official updater verifier before
emitting the file. Retain both prepared output and provenance as artifacts of
that run. Feed their downloaded bytes directly into the promotion inventory.

This is a concrete additional step for the existing native packaging recipe,
not a new enabled workflow. A reviewed future native job must supply the final
release signing evidence and invoke `prepare` as described in the distribution
guide; current unsigned/default builds do not qualify. `record-build --fixture`
is available for isolated synthetic receipts and labels its output as fixture.
That output cannot fill a production evidence slot.

The compiled-route fields attest the **same-job build-variable contract**; the
command does not decode the executable to independently authenticate its compiled
constants. Trusted native CI must actually build the final application using
those unchanged variables. Never call it with unrelated build inputs and describe
that as proof of the packaged binary. Device startup/log evidence remains
separate.

The command checks file hashes and matching fields. JSON and a GitHub run URL
alone do not cryptographically authenticate a native build or prove physical
device work. Obtain provenance from the actual trusted run, review its build
inputs and retained output, and protect the promotion job's inputs. Never author
an invented successful provenance record to retrofit a legacy receipt. Do not
use a fixture run as production provenance.

Keep automated tests, native signatures, deployment/public readbacks, and actual
devices as separate evidence sections. Every passed result needs nonempty HTTPS
references to concrete redacted results or logs. Record UTC operator/date, OS
version/architecture, older installed version and executable/install path, and
completed installation plus startup of the accepted new version from the
expected executable. Installer launch, process exit and a unit-test process are
insufficient. The startup record binds the updater artifact digest that supplied
the installed version; its log should independently establish installed build
identity.

For **each target**, complete every template check: signed-out and zero balance;
automatic/manual discovery, release notes and separate download/install consent;
Later and exact-version ignore across actual process restarts, ignored manual
checks and newer reminders; desktop and independently installed CLI mutations,
refusal/retry and the guard-to-installer race without cancelling work; retained
Client Session, credential identity, Tool Connections, billing owner, models,
user configuration and restoration records; independent CLI use afterward;
network, invalid-package and installer failure/retry; independently opened
external recovery instructions; legacy manual bootstrap then in-app update.
Use synthetic scopes, hashes and redacted references, never real credential
values. Do not reset real state, rotate keys or make paid model requests.

## Protected destination and serialization

All future real candidate and promotion jobs writing this OSS destination must
use the **same literal workflow-level group**, across workflow filenames:

```yaml
concurrency:
  group: token-market-client-update-distribution
  cancel-in-progress: false
```

The manual fixture workflow already uses this group but has no release secrets
and publishes only to temporary local stores. Do not use per-workflow groups,
`${{ github.workflow }}`, run IDs, branch names or cancel in progress; those
allow two writers or interrupt the manifest transition. All writers must be in
the same protected GitHub repository. This serialized workflow contract is
required because a runner-local lock cannot serialize OSS across runners.

After real provisioning, restrict a protected main workflow and environment to
the reviewed publisher, trusted inputs and least-privilege upload identity.
Set `TOKEN_MARKET_UPDATE_PUBLISH_DESTINATION` to the **exact**
`oss://BUCKET/PREFIX`; set `TOKEN_MARKET_UPDATE_PROMOTION_WORKFLOW_REF` to the exact
`OWNER/REPOSITORY/.github/workflows/FILE.yml@refs/heads/main`. Within that serialized
job set `TOKEN_MARKET_UPDATE_PUBLISH_CONCURRENCY` to the literal group above.
`promote` checks the destination against the configured bucket/prefix and checks
`GITHUB_ACTIONS`, `GITHUB_REPOSITORY` and `GITHUB_WORKFLOW_REF` against that protected
workflow. These environment checks enforce the deployment contract inside trusted
CI; environment variables are not a distributed lock or authentication of an
untrusted operator. Do not enable a live writer while configuration remains empty.

## Read-only validation and publication order

Inside the configured protected job, first run:

```sh
python3 scripts/desktop-update-release.py promote \
  --inventory "$INVENTORY" --acceptance "$ACCEPTANCE" \
  --verifier "$UPDATER_VERIFIER" --dry-run
```

Dry-run performs the same destination, schema, provenance and artifact validation
and real official signature/signed-version verification using disposable local
snapshots. It does not invoke OSS, read public objects, acquire destination locks
or write distribution files. Missing configuration reports field names. Missing,
fixture, incomplete or mismatched acceptance fails before any real write. Dry-run
success means the submitted evidence passed validation, not independent device
observation or authorization to publish.

Only after actual acceptance and deployment are complete may the protected job
invoke the same command without `--dry-run`, supplying pinned `--ossutil`. It
reuses the candidate publisher's verified snapshots, ossutil2.4.1 adapter,
immutable object keys and anonymous public readbacks:

1. Upload/reuse all version/digest updater and manual-installer objects; GET
   verifies exact SHA-256, size, type, no content transformation and immutable
   cache policy. Existing candidates are reused and public-verified.
2. Upload and public-verify immutable version/digest recovery HTML.
3. PUT those same HTML bytes to the fixed recovery alias with discovery no-cache
   headers; public-verify that alias before any stable manifest write.
4. Perform the single final production stable manifest PUT and public GET.

Any failure before step 4 leaves previous discovery bytes unchanged. Failure
may leave additional immutable objects or the accepted newer recovery alias;
retain old immutable artifacts/pages. If final manifest PUT succeeds and its GET
fails, discovery **may already have advanced**. Inspect public state before
retrying; an error is not a rollback guarantee. Never delete the previous release
or reset client data to recover.

The fixed alias must also be linked from official external download/support
navigation. Actual independent browser opening is a separately recorded device
check; a successful publisher GET cannot fill that slot.

## Isolated fixtures and remaining delivery

`promote --fixture --local-store "$TEMP_STORE"` uses production-shaped object keys
only in that temporary store. It requires `fixture: true`, synthetic device
records and real fixture updater signatures. It cannot use OSS, even if all OSS
credentials are populated. Output says `mode: synthetic-fixture` and
`actual_device_acceptance: false`. A fixture dry-run uses the same local-only
classification. Never copy its acceptance/build evidence into production.

The fixture suite exercises the public CLI with the official updater verifier
and isolated HTTP/store. It does not invoke physical installers. Native lifecycle
tests separately cover guards, credential scopes and installed startup semantics.
The actual older-installed-client HTTPS upgrade and recovery scenarios on both
required device types remain open, along with provisioned distribution and an
authorized stable promotion. Keep those statuses separate in the release handoff.
