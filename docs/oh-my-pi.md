# Oh My Pi

Token Market supports the official **Oh My Pi 18.15.0**, command `omp`, from
`can1357/oh-my-pi`. This is distinct from Pi (`pi`). Install with the app's
**安装** action or `tokenmarket install oh-my-pi`, then explicitly connect.

The installer uses fixed official release binaries and SHA256 values for macOS,
Windows and Linux on arm64/x64, with separate musl Linux builds. The standalone
binary embeds its runtime, native helpers and Brush shell; Bun, Node and Git Bash
are not prerequisites. A staged `--version` probe must succeed before publishing
`~/.tokenmarket/tools/oh-my-pi/bin/omp` (`omp.exe` on Windows). Windows needs
PowerShell >=5.1. POSIX checksum verification needs sha256sum or shasum.
On Alpine/musl the official binary also needs system `libstdc++` and `libgcc`.
A missing runtime fails installation with `apk add libstdc++ libgcc` guidance and
does not publish a broken command. Platform-native acceptance remains a release check.

The default models file is `~/.omp/agent/models.yml`. Existing `models.yaml` is
used when the canonical filename is absent. `PI_CODING_AGENT_DIR` relocates the
default agent directory; this variable name is shared with Pi, whose file remains
`models.json`. A named `OMP_PROFILE` selects `~/.omp/profiles/<name>/agent` and
ignores the agent-directory override. `PI_PROFILE` is consulted only if
`OMP_PROFILE` is absent; explicit empty/default selects the default profile.
`PI_CONFIG_DIR` changes the config root dirname. Only the active profile is managed.
An existing legacy `models.json` requires OMP's own migration before connecting.

Connect imports the Key-visible TEXT_GENERATION catalogue under
`providers.tokenmarket`, using the gateway `/v1`, `api: openai-completions`,
`auth: apiKey` and Bearer authentication. It tracks individual ID-selected model
fields, preserving other models, unknown fields and array order. Default roles
in `config.yml`/`config.yaml` and existing authentication databases are untouched.
Use `/model` to select a model. Reconnect reuses the saved scoped Key; Disconnect
restores owned fields while retaining later user edits.

OMP's inline config Key outranks stored OAuth in `agent.db`, including broker
OAuth. Ordinary broker clients with required credentials remain supported;
missing broker tokens, disabled Token Market providers, competing auth headers,
`pi-native` gateway transport and conflicting model override routes stop before
issuance. A broker-backed gateway server can deliberately ignore local models;
Token Market launches ordinary `omp`, not `auth-gateway serve`. Runtime CLI
`--api-key`, `--profile` and `--config` overlays belong to that explicit invocation.
Key commands are never executed by Token Market; a leading `!` secret or conflicting
same-name environment variable cannot be silently written as a literal Key.

Status and Connection Test revalidate effective settings and profile. Testing
checks one explicitly selected model without changing roles or config. It proves
text protocol output only; real streaming and tool calls require separate Agent
acceptance. Installation and Connect do not send model requests.

Official tagged sources, checked 2026-11-02:

- [Stable release, native binaries and SHA256s](https://github.com/can1357/oh-my-pi/releases/tag/v18.15.0)
- [Official binary installer and musl requirements](https://github.com/can1357/oh-my-pi/blob/v18.15.0/scripts/install.sh)
- [Windows installer prerequisites](https://github.com/can1357/oh-my-pi/blob/v18.15.0/scripts/install.ps1)
- [Profiles and settings](https://github.com/can1357/oh-my-pi/blob/v18.15.0/docs/settings.md)
- [Profile path resolution source](https://github.com/can1357/oh-my-pi/blob/v18.15.0/packages/utils/src/dirs.ts)
- [Provider authentication precedence](https://github.com/can1357/oh-my-pi/blob/v18.15.0/docs/providers.md)
- [Model schema and credentials](https://github.com/can1357/oh-my-pi/blob/v18.15.0/docs/models.md)
- [Gateway-only local-config suppression](https://github.com/can1357/oh-my-pi/blob/v18.15.0/packages/coding-agent/src/cli/auth-gateway-cli.ts)
