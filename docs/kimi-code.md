# Kimi Code

Token Market supports the official TypeScript **Kimi Code 2.4.1**, command `kimi`,
npm identity `@moonshot-ai/kimi-code`. It installs fixed official standalone release
archives for macOS/Linux/Windows on arm64/x64, checking published SHA256 values.
The native Node SEA binary embeds its runtime and native assets, so Node is not
needed. The npm alternative requires Node >=22.19. This integration does not use
the legacy Python/uv product or mutate its `~/.kimi` data.

Install with **安装** or `tokenmarket install kimi-code`, then explicitly Connect.
A staged `--version` must work before publishing the private command under
`~/.tokenmarket/tools/kimi-code/bin`. POSIX needs Bash, tar and sha256sum/shasum.
Windows needs Git Bash: an existing shell or explicit valid `KIMI_SHELL_PATH` is
reused; otherwise the shared dependency helper verifies and extracts official
portable Git for Windows 2.56.0.4. Pi and Kimi reuse the same private runtime,
including existing legacy Pi Git paths. Separate download files, a dependency
mutex and completion marker coordinate concurrent preparation. No administrator
access or persistent PATH change is required. Launching Kimi supplies the resolved
private Bash path without replacing an explicit user override.

Read-only inventory never runs version probes. Explicit Connect rejects a positively
identified Kimi 0.x/1.x command before issuance or config mutation: this adapter
uses the new 2.x contract. Unknown probe metadata is not proof of incompatibility.
No automatic upgrade, downgrade or legacy migration occurs. If needed, choose the
current official product manually; Kimi's own documented migration is separate.

Configuration is `$KIMI_CODE_HOME/config.toml`, otherwise
`~/.kimi-code/config.toml`. Connect writes `[providers.tokenmarket]` with
`type="openai"`, the model API `/v1` URL and a literal `api_key`. It imports the
Key-visible TEXT_GENERATION catalogue as `[models."tokenmarket/<id>"]` entries.
Every model needs a positive `max_context_size`, from real catalogue
`context_window_tokens` or a valid existing entry for the same provider/model.
Missing data stops before config publication. The shared issuance protocol first
saves/confirms the Key so it can read that exact Key's catalogue; a metadata failure
retains that saved Key, and retry reuses it rather than issuing another one.

Eligible Agent models explicitly gain `tool_use`. Confirmed model metadata also
refreshes thinking/always-thinking and image/video/audio input tags, independent
input limits and supported/default/off effort fields. Unrelated capability tags
remain. This follows Token Market's TEXT_GENERATION eligibility contract, and does
not prove native Kimi tool calls. Actual Agent streaming/tool calling remains
release acceptance. The adapter leaves `default_model`, global `[thinking]`, other
providers, unrelated user fields and TOML comments intact. Model selection stays
inside Kimi. See [all-tool model projection](model-capabilities.md#全工具配置投影). OpenAI does not read Kimi's Anthropic-only `max_output_size`, so the adapter
does not fabricate an output limit or claim that field constrains this protocol.

Declared `api_key_env`, OAuth, dynamic registry sources, competing authorization
headers, model-level request routes, `KIMI_MODEL_NAME`, and effective auth header
environment overrides are surfaced before takeover. Inline keys are literal;
unrelated ambient `OPENAI_API_KEY`/`KIMI_API_KEY` values are not implicit fallbacks.
Reconnect reuses the scoped Key and restores retired fields. Disconnect restores
managed fields under the standard edit-preservation/conflict rules. Connection
Test sends one selected-model text request and leaves configuration unchanged;
installation and Connect send no model requests.

Official stable sources, checked 2026-11-02:

- [Release and artifact SHA256s](https://github.com/MoonshotAI/kimi-code/releases/tag/%40moonshot-ai%2Fkimi-code%402.4.1)
- [Released single-file archive layout](https://github.com/MoonshotAI/kimi-code/blob/2c73f3f50af4fa7f2540ca0d2e33b07a1408e3a7/apps/kimi-code/scripts/native/produce-manifest.mjs)
- [Provider protocol and credential precedence](https://github.com/MoonshotAI/kimi-code/blob/2c73f3f50af4fa7f2540ca0d2e33b07a1408e3a7/docs/en/configuration/providers.md)
- [Config schema and model requirements](https://github.com/MoonshotAI/kimi-code/blob/2c73f3f50af4fa7f2540ca0d2e33b07a1408e3a7/docs/en/configuration/config-files.md)
- [Environment and shell overrides](https://github.com/MoonshotAI/kimi-code/blob/2c73f3f50af4fa7f2540ca0d2e33b07a1408e3a7/docs/en/configuration/env-vars.md)
- [Official legacy migration boundary](https://github.com/MoonshotAI/kimi-code/blob/2c73f3f50af4fa7f2540ca0d2e33b07a1408e3a7/docs/en/guides/migration.md)
- [Portable Git runtime and digests](https://github.com/git-for-windows/git/releases/tag/v2.56.0.windows.4)
