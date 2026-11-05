# Distribute desktop updates through a project-controlled static endpoint

On 2026-11-05 we chose a fixed HTTPS update address under a project-controlled
domain, backed by a static version manifest and domestically accessible object
storage/CDN, with GitHub retained for builds and release archives. Production
clients initially receive only stable releases; test builds use separate links.
This adds hosting and publishing work in exchange for controlling the update
address embedded in installed clients and their download availability, while
deferring a dynamic update service until release policy requires one.

This is an accepted distribution decision, not evidence that hosting or an
updater is deployed. The confirmed behavior and remaining delivery work are tracked in the
[desktop update design](../desktop-client-updates.md).
