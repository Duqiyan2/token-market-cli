# Pi

Token Market manages **Pi** (`pi`), separately from Oh My Pi. It installs the
compatible official stable **1.4.0** standalone archive for macOS, Linux and
Windows on arm64/x64. The archive includes Bun, native platform helpers and data;
the complete layout is retained under `~/.tokenmarket/tools/pi/runtime`. Each
artifact is checked against its fixed official SHA256 before extraction. The
npm alternative is `@earendil-works/pi-coding-agent` and requires Node >=22.19;
Token Market uses the standalone route and does not need Node.

POSIX installation needs Bash, tar and sha256sum or shasum. A missing Bash produces
an actionable installation failure. On Windows, an existing Bash is reused. If
none is discovered, the installer downloads and verifies the official portable
Git for Windows **2.56.0.4** into a private directory, without administrator access
or persistent PATH changes. Token Market's launch environment includes its Bash.
Installation preserves configuration and never starts Pi, connects or tests a model.

Pi reads `PI_CODING_AGENT_DIR/models.json`, otherwise `~/.pi/agent/models.json`.
Connect imports the Key-visible text-generation catalogue under
`providers.tokenmarket`, using `openai-completions`, the model API `/v1` URL, Bearer
authentication and ID-selected model rows. Existing `settings.json` choices remain
untouched. Open `/model` in Pi to choose a model after connecting.

A stored `auth.json` credential takes precedence over `models.json.apiKey`.
Token Market therefore refuses an existing `tokenmarket` auth entry and asks the
user to `/logout` that provider first. It preserves every auth entry, including
OAuth credentials. Competing authorization headers and provider OAuth are also
rejected. Key recovery decodes literal escaping only; it never runs key commands
or substitutes environment references. Explicit Pi `--api-key` runtime overrides
remain the responsibility of whoever starts Pi with that flag.

Reconnect reuses the scoped saved credential and refreshes the compatible catalogue.
Disconnect restores only managed fields, preserving model order, user models and
unknown fields. Connection Test sends one explicitly selected-model text request
without changing the default or configuration. It does not prove streaming or
tool calls. Native installation/runtime and paid Agent acceptance remain release checks.

Official sources, checked 2026-11-02:

- [Stable release and artifact checksums](https://github.com/earendil-works/pi/releases/tag/v1.4.0)
- [Tagged binary build and archive layout](https://github.com/earendil-works/pi/blob/v1.4.0/scripts/build-binaries.sh)
- [Package and Node requirement](https://github.com/earendil-works/pi/blob/v1.4.0/packages/coding-agent/README.md)
- [Configuration directory](https://github.com/earendil-works/pi/blob/v1.4.0/packages/coding-agent/docs/configuration.md)
- [Provider schema](https://github.com/earendil-works/pi/blob/v1.4.0/packages/coding-agent/docs/models-configuration.md)
- [Authentication precedence and model selection](https://github.com/earendil-works/pi/blob/v1.4.0/packages/coding-agent/docs/models.md)
- [Windows shell requirements](https://github.com/earendil-works/pi/blob/v1.4.0/packages/coding-agent/docs/windows.md)
- [Private portable Git dependency and hashes](https://github.com/git-for-windows/git/releases/tag/v2.56.0.windows.4)
