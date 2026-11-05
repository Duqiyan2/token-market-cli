# Follow CC Switch for tool lifecycle policy

On 2026-11-04 the user replaced the previous npm-first and pinned-install policies
with the display, install, upgrade and uninstall rules examined in CC Switch
[`e217bfc`](https://github.com/farion1231/cc-switch/commit/e217bfcbbbbbf82f4619ab62f08fb002635b7858).
New CLI installations follow the official per-tool route and latest channel;
existing installations follow terminal precedence and their original update
channel. Uninstall targets a specific proven installation and retains configuration,
user data and saved connections. This trades private, reproducible installers for
upstream lifecycle behavior; it does not change the supported tool catalogue or
make a lifecycle operation a connection/model test. See the executable
[policy](../../rust/tokenmarket-core/src/installation/policy.rs) and
[lifecycle contract](../tool-lifecycle.md).
