# Distribute desktop updates through a project-controlled static endpoint

On 2026-11-05 we chose Tauri updater signatures and a static stable manifest in
Alibaba Cloud OSS. On 2026-11-06 the operator workflow was reconciled with the
first release and the user's confirmed local-only, agent-operated publication.

The current public domain is `https://dl.tokensapi.cn`. The website links to a
static OS download entry and fixed installer names. Application updates continue
using a versioned package URL and signature from the stable JSON manifest. Existing
compatible OSS URLs are retained; no further application-internal path shortening
is required. The recovery HTML page is not part of the current release flow.

Operators trigger `scripts/release-desktop.py` locally. It reuses native Mac
signing, supports Windows currentUser NSIS cross-compilation from Mac, stages the
nine public files, writes their OSS metadata, verifies anonymous reads and writes
stable last. The optional native Windows signing recipe and stricter device
acceptance tooling remain separate advanced paths. They must not be represented
as evidence that cross-built packages have been Authenticode-signed or exercised
on Windows.

All actual configuration, keys, certificates, OSS profiles, outputs and evidence
stay outside Git. GitHub does not hold release secrets and is not the publishing
host. A designated account/host owns writes; local OS locks are not distributed
locks. Build, platform signing/notarization, updater verification, publication and
actual device upgrades remain separate evidence levels. Package publication alone
does not complete the pending device acceptance work.

See the [release runbook](../client-update-local-release.zh-CN.md) and
[distribution module contract](../desktop-update-distribution.md).
