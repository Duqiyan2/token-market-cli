# Desktop tool management

CLI and desktop use the same connection manager and local restoration records.
See [client login](desktop-login.md), [tool discovery](tool-inventory.md) and
[shared credentials](adr/0004-shared-client-key-lifecycle.md).

## Connection and model configuration

Current Workspace controls the next Connect or Reconnect. Existing connections
retain their account and Bound Workspace. Changing the current workspace or
logging out never rewrites Agent configuration.

Claude Code uses Anthropic Messages and Codex uses Responses, with the client's
designated default text model. Other adapters use compatible Chat Completions
models. DuMate Personal configures one model; multi-model tools import eligible
text models without replacing existing model preferences. Model eligibility
comes from `model_types` containing `TEXT_GENERATION`; missing required models
stop connection without changing the configuration.

Missing model metadata is supplemented from the bundled [model capability
catalogue](model-capabilities.md), while valid server values retain priority.
Harness also receives per-model reasoning controls and protocol compatibility.

Connect saves the credential, loads the model catalogue and publishes managed
configuration with encrypted restoration records. It sends no generation request.
Reconnect validates and reuses a saved credential when possible. A different
account or billing workspace requires explicit confirmation.

Claude Code manages `ANTHROPIC_MODEL`, the
`ANTHROPIC_DEFAULT_{FABLE,HAIKU,OPUS,SONNET}_MODEL` and `_MODEL_NAME` fields, and
`CLAUDE_CODE_AUTO_MODE_SERVER` set to the string `"0"`. Each managed field retains
its original value for restoration.

DuMate must be fully closed before configuration changes. Its adapters update
account-specific preferences, not generated runtime files or login settings.
See [Personal](dumate.md) and [Enterprise](dumate-enterprise.md) integration notes.

## Storage and restoration

Credentials, connection receipts and owned backups are encrypted at rest. Native
credential stores protect the local encryption key; Linux supports a passphrase
vault when Secret Service is unavailable. An unreadable store returns an error.
Status summaries contain no credentials. Tool configuration contains the
credential the target tool needs to run.

Disconnect restores only owned fields. Preserve unknown settings and later edits;
conflicts require a current, explicit user choice. Never restore a whole file
from an old backup over newer work. OpenCode JSONC edits preserve unrelated
comments, trailing commas and unknown fields.

## Explicit model testing

Test requires fee confirmation and one configured model. Before sending a
request, check managed configuration against the receipt. Configuration drift,
missing files or conflicting credentials disable testing until repaired.

Use the saved connection's key, Bound Workspace, API address and protocol. A pass
proves one complete non-streaming text response, rather than streaming, tool calls
or every imported model. Reconfiguration starts an untested connection.

## Launch and installation

Open Tool resolves fresh executable/application discovery. Terminal launch
includes newly discovered PATH entries; fixed tool commands and verified
application paths are used. Reopen running tools after configuration changes.
OpenClaw also needs its session restarted. OpenCode project configuration can
override global settings.

Installation has its own progress, cancellation and retry. It neither connects a
tool nor initiates testing. See [graphical installation](gui-installation.md).

## Local verification

```sh
cargo fmt --manifest-path rust/tokenmarket-core/Cargo.toml --all --check
cargo fmt --manifest-path rust/tokenmarket-cli/Cargo.toml --all --check
cargo fmt --manifest-path src-tauri/Cargo.toml --all --check
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --all-targets --locked
cargo test --manifest-path rust/tokenmarket-cli/Cargo.toml --all-targets --locked
cargo check --manifest-path src-tauri/Cargo.toml --locked
```

Fixtures use temporary configuration and local fake HTTP. Installed-app login,
credential-store access, actual installation, tool launch and paid model use need
separate native verification. The [model catalogue checker](../scripts/check-model-catalogue.py)
can inspect a privately saved `/v1/models` response without sending a request.
