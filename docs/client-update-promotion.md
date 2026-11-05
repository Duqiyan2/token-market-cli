# Accepted desktop release promotion

`local-validate` and `local-publish` consume the same local schema2 acceptance
record. The former is read-only; the latter explicitly publishes exact accepted
bytes to the configured OSS destination. Use the [local config example](client-update-local.example.json)
and [Chinese release guide](client-update-local-release.zh-CN.md). Production
resources and both actual devices remain external #38 work; no Windows device
acceptance is claimed. Parent #30 remains open.

## Candidate and installed acceptance clients

Choose the final HTTPS production manifest and fixed recovery alias, plus the
persistent updater public key, **before** building. Both final targets compile
those exact production routes/key. Publish those bytes to a separate
`test/.../stable.json` with `local-candidate`; this never advances public stable.

Install an older, internal updater-enabled acceptance client configured to
discover that isolated candidate URL. Record its source/configuration, lower
version and install path in evidence. Its endpoint differs from the **candidate's**
compiled production endpoint. Upgrade to the exact candidate and verify the new
installed process; do not rebuild the candidate or patch its endpoint after
acceptance. Separately test a legacy client without an updater by manual bootstrap
and a subsequent in-app update. Windows acceptance starts from a qualifying
currentUser NSIS installation; MSI ownership stays manual/recovery.

## Exact local records

`local-build` emits `prepared.json` and `build.json` from an actual native recipe
invocation; `local-inventory` consumes both outputs without manual signature edits.
The [build record template](client-update-build-provenance.template.json) documents
its schema; never fill it manually to replace a missing build. Schema2 includes
`provenance_kind: local-native-build`, `fixture: false`, full source commit, stable
version, target, artifact hashes/sizes, prepared hash, certificate SHA1, compiled
endpoint/recovery/key fingerprint, shared public configuration hash, host, recipe
and start/completion times. Different private paths on Mac/Windows are allowed;
their source/version/public configuration must agree. Old CI/schema1 or fixture
records are refused. Mac explicitly retains its actual notarization boolean.

Copy [the acceptance template](client-update-acceptance.template.json) outside
Git. Its `schema_version: 2`, `provenance_kind: local-operator-acceptance` and
`fixture: false` identify local operator acceptance. Both `actual_device` fields
start false, every check is pending, and the empty template cannot pass. Unknown,
missing, wrong-type or mismatched fields fail closed.

The release binds source commit, version, the complete final updater/manual
artifact set and SHA256 of the exact pretty-printed candidate manifest bytes.
The routes bind public base, isolated candidate URL, production manifest URL,
fixed recovery URL and public-key SHA256. Each actual native device binds:

- Operator, UTC test time, OS/architecture, lower starting version, updater-enabled
  starting install path and successful real installed upgrade.
- New executable path, started version and exact updater artifact digest.
- Original build/prepared record hashes, native signing certificate identity and
  the candidate's compiled production routes/key.
- Every check in the template: discovery/notes/consent, Later/ignore across restarts,
  signed-out/zero-balance operation, desktop/CLI mutation exclusion and races,
  active work retention, Client Session/credentials/Tool Connections/billing/models/
  user configuration/restoration records, independent CLI use, failure/retry paths,
  external recovery navigation and legacy bootstrap.

Each evidence result uses `{"status":"passed","evidence":[...]}`; startup also
includes its version/executable/artifact hash. References may be real HTTPS logs,
or a private absolute outside-Git file bound by digest:

```json
{"path":"/absolute/private/evidence/installed-upgrade.txt","sha256":"FULL_FILE_SHA256"}
```

The CLI reads that exact file and verifies its hash. Empty, changed, missing,
synthetic or example HTTPS evidence cannot substitute for a real record. File
hashes prevent accidental changes; they do not independently authenticate an
operator's claims or turn synthetic results into device attestation. Preserve
original logs/screenshots and review them honestly. The CLI does not launch an
installer or exercise a device for the operator.

## Validate and explicitly publish

```sh
python3 scripts/desktop-update-release.py local-validate \
  --config "$PRIVATE_CONFIG" --inventory "$INVENTORY" --acceptance "$ACCEPTANCE"
python3 scripts/desktop-update-release.py local-publish \
  --config "$PRIVATE_CONFIG" --inventory "$INVENTORY" --acceptance "$ACCEPTANCE"
```

Validation uses finalized artifact snapshots and the real official updater
signature/signed-version verifier. It does not read credential contents, contact
OSS/public routes, acquire a publisher lock or upload anything. Publication uses
the same checks plus the shared OS destination lock, current stable preflight,
immutable uploads/readbacks, fixed recovery alias readback, then stable PUT/readback
last. See [distribution details](desktop-update-distribution.md). An error after
stable PUT can mean discovery advanced; inspect it before retrying. Keep old
installers and shared user data; no automatic rollback or arbitrary data downgrade
compatibility is promised.
