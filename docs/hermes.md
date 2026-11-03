# Hermes CLI adapter

Verified against official Hermes Agent **v0.22.5** on 2026-11-02. The stable
annotated tag resolves to commit `bf0203a794a07aac3ff764972ab9087baa4e4f4e`.
The canonical Token Market tool ID and executable name are both `hermes`.

## Installation

The CLI supports macOS Intel/Apple Silicon, Linux x64/arm64 and native Windows
x64/arm64. Hermes desktop package availability is a separate matrix. Other
architectures retain a disabled Install action with a reason.

Token Market downloads the official source bootstrap script from the immutable
commit, verifies SHA256 before loading it, and places the checkout under
`~/.tokenmarket/tools/hermes/checkout`. The pinned scripts have SHA256:

- POSIX: `99cb3691d55019c6973a7acb775a915648666cc24e483ca827a1cdbb3531a674`
- PowerShell: `8d491a8b9b19934bb8d027b11962c394551798e969d244b68d4ab95295b970e2`

The upstream bootstrap stages prepare prerequisites, repository, managed Python
and Python dependencies. Then upstream PM selects the committed interpreter;
upstream product build, source stamp and launcher publication run under its
update lock. CLI products are built without the desktop application, optional
browser/computer-use setup, provider setup or gateway service setup. Existing
compatible 0.22.x CLI installations are reused; incompatible versions are not
upgraded automatically. Post-install success still requires command discovery
and the exact pinned version.

This deliberately skips the upstream complete-source tail's post-update config
migration and update-channel mutation. Even noninteractive full installers run
that maintenance, which would conflict with the requirement to preserve existing
user configuration during Install. Completed dependencies survive failure or
cancellation; retry uses the upstream staged bootstrap. Install and Connect are
separate user actions.

## Configuration and credential precedence

The active file is `$HERMES_HOME/config.yaml` when HERMES_HOME is set. Defaults
are `~/.hermes/config.yaml` on macOS/Linux and
`%LOCALAPPDATA%/hermes/config.yaml` on Windows (fallback
`%USERPROFILE%/AppData/Local/hermes/config.yaml`). Inventory reads files and
command evidence without executing a version probe. Explicit Connect performs
the compatibility check.

Connect imports all compatible text models into the YAML dictionary
`providers.tokenmarket.models`, sets `api` to the model API `/v1`, an inline
`api_key`, and `transport: chat_completions`. Existing `api_mode`/`apiMode`
aliases are owned and restored too because they outrank transport. Each model's
`context_length` is managed independently. A model without advertised context
uses YAML null, which Hermes treats as unspecified/auto-detected. User-added
models and unknown row fields are retained. `model.provider`, `model.default`
and `default_model` stay under the user's control; select Token Market in Hermes'
`/model` interface when desired. CLI route/key overrides remain the user's
responsibility.

Hermes executes `key_cmd` and supports environment-backed credentials; saved
`auth.json` credential pools can win over an inline key. The adapter refuses
competing declared credential sources, Authorization headers, disabled providers,
conflicting selected-model overrides and legacy same-name providers before Key
issuance. Sibling `auth.json` and a profile's root fallback are read-only. A
competing Token Market pool or malformed auth store must be resolved explicitly;
Hermes' own legacy config-seeded rows can be refreshed from the inline key.
The adapter never executes key helpers or modifies auth.json/.env.

Shared connection receipts own individual configuration fields and newly created
containers. Reconnect reuses the bound scope's saved Key when valid, refreshes
models and retires removed managed leaves. Disconnect restores original managed
values while retaining later manual modifications. Connection Test is an
explicit lightweight Chat Completions check; it leaves model preference intact.

## Evidence and remaining native acceptance

Public management fixtures cover installation planning/execution boundaries,
command re-detection, platform paths, full text catalogue filtering, Key reuse,
read-only credential conflict failures, lightweight testing and restoration after
manual edits. Desktop fixtures cover the Hermes identity, icon, independent
Install/Connect flow and no implicit paid test.

These fixtures do not execute downloaded third-party code. Real dependency
bootstrap/installation on each OS, actual Hermes launch/config interpretation,
streaming conversation and a tool call remain release acceptance work. No native
installation or paid model call was performed for this change.

Official evidence:
[stable release](https://github.com/NousResearch/hermes-agent/releases/tag/v0.22.5),
[pinned POSIX bootstrap](https://github.com/NousResearch/hermes-agent/blob/bf0203a794a07aac3ff764972ab9087baa4e4f4e/scripts/install.sh),
[pinned Windows bootstrap](https://github.com/NousResearch/hermes-agent/blob/bf0203a794a07aac3ff764972ab9087baa4e4f4e/scripts/install.ps1),
[provider normalization](https://github.com/NousResearch/hermes-agent/blob/bf0203a794a07aac3ff764972ab9087baa4e4f4e/hermes_cli/config_providers.py),
[runtime credential selection](https://github.com/NousResearch/hermes-agent/blob/bf0203a794a07aac3ff764972ab9087baa4e4f4e/hermes_cli/runtime_provider_custom.py),
[auth stores](https://github.com/NousResearch/hermes-agent/blob/bf0203a794a07aac3ff764972ab9087baa4e4f4e/hermes_cli/auth.py),
[platform homes](https://github.com/NousResearch/hermes-agent/blob/bf0203a794a07aac3ff764972ab9087baa4e4f4e/hermes_constants.py).
