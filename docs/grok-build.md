# Grok Build integration

Grok Build is the official `grok` CLI from xAI. The installation catalogue pins
the official stable version `1.0.70`, read from `https://x.ai/cli/stable` on
2026-11-02. Its official shell/PowerShell installer accepts an exact version.
The client puts the visible command in a user-owned managed directory and
re-detects the actual executable/version after installation. Existing compatible
installations are reused; installing never establishes a Tool Connection.

Configuration follows the upstream
[custom-model contract](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/11-custom-models.md):
the effective `GROK_HOME` (default `~/.grok`) contains `config.toml`, with a
`model` table per namespaced Token Market model. Each managed entry sets the
actual model ID, display name, Chat Completions backend, gateway and per-model
API key. Advertised context/output limits are copied when present. The existing
`models.default`, other model tables, preferences and comments are retained.

Per-model API keys take precedence over a saved Grok login, so connecting does
not overwrite login state. Conflicting explicit Authorization headers or auth
helpers on the managed entries stop connection before key issuance. Managed
field receipts cover reconnect, model removal and restoration; later unrelated
edits survive disconnect. Installation clears only its child process's inherited
deployment-key setting because the upstream installer otherwise uses that value
to configure inference; existing credentials and connection files are untouched.

The public manager fixtures verify catalogue import, key reuse, configuration
preservation, explicit lightweight testing, and restoration. Temporary installer
fixtures verify executable re-detection, and desktop fixtures verify separate
Install/Connect/Launch actions. These checks do not establish native installation,
the deployed platform's tool-ID acceptance, streaming, or tool invocation inside
the actual Grok runtime. Those remain the separate release acceptance boundary.
