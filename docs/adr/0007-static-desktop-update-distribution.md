# Distribute desktop updates through a project-controlled static endpoint

On 2026-11-05 we chose a fixed HTTPS address under a project-controlled domain,
a static stable manifest, updater-signed artifacts and immutable version/digest
paths in Alibaba Cloud OSS. The confirmed local release amendment replaces the
unreleased CI-only publisher: operators build and sign on native hosts, then one
designated local publishing machine validates candidates and explicitly promotes
the accepted bytes. Private configuration, persistent identities, credential
profiles, artifacts and evidence stay outside Git; GitHub need not hold release
secrets, and neither R2 nor a GitHub download fallback is required.

This trades distributed CI publishing for an operator-controlled local workflow.
A destination-specific OS lock serializes local candidate/stable writers across
checkouts on the publishing account; it is not a multi-host lock. Other hosts and
unprovisioned CI must not write this destination. Local build records describe
observed native recipe invocations and final verified bytes, not cryptographic
device attestation. Both native targets and actual installed-client acceptance
remain mandatory; this decision does not establish deployment or device success.
See the [local release guide](../client-update-local-release.zh-CN.md).
