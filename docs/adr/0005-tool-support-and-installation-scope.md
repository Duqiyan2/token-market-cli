# Tool support includes installation and runtime verification

Date: 2026-11-02

## Status

Accepted product decisions, synthesized for issue publication at the user's
explicit request through `to-spec`. Individual installation/protocol
compatibility checks remain implementation work; this document is not
implementation or verification evidence.

Published specification: [issue #13](https://github.com/Duqiyan2/token-market-cli/issues/13),
labelled `ready-for-agent`. Publication used the configured
`FitBBC/token-market-cli` issue tracker; GitHub returned this canonical URL.

## Accepted scope

Retain Claude Code, Codex CLI, WorkBuddy, DeepSeek Harness, OpenClaw and OpenCode,
and add Grok Build, Hermes, Pi, MiniMax Code, Kimi Code and Oh My Pi. The final
scope is twelve tools: six existing and six new integrations. Tool names do not
establish verified upstream identities or third-party gateway compatibility.

The user deferred Gemini CLI integration because Token Market has no supported
models for it, and subsequently deferred Antigravity CLI as well. Neither belongs
in this round's tool-support work. Q9's Gemini-family initial-selection proposal
and Q10's Gemini experimental-menu proposal are withdrawn without acceptance.

The user explicitly requires an Install button whenever any in-scope supported
tool is not installed, including the six existing tools. Support includes installation
through a verified official method, installation detection, Token Market
connection, compatible model configuration, explicitly triggered testing, and
disconnection with configuration restoration. Merely opening a download page
does not satisfy the installation goal. Upgrade and uninstall management are
outside this addition's scope.

Acceptance requires running each actual tool against Token Market and verifying
streaming conversation and a tool call. Configuration writes, a basic protocol
test, and actual tool runtime verification are separate evidence. Installation
and connection do not automatically trigger paid tests; tests remain an explicit
user action.

The existing shared CLI/desktop connection lifecycle, workspace/key ownership,
configuration preservation and supported client platform boundaries remain in
force under ADR 0004. This decision expands the tool and installation scope from
ADR 0003; it does not replace those lifecycle rules.

The existing model policy remains in force: Claude Code and Codex use their
established required-model policy; other tools receive the complete compatible
text-generation catalogue while preserving existing model preferences. No new
default-model selection exception was accepted during this interview.

## Accepted installation behavior

- Q3: prepare missing dependencies as part of installation, reuse compatible
  existing versions, and prefer a user-level or tool-private installation.
  Required system authorization uses the operating system's normal prompt.
- Q4: for GUI distribution, download and open the official installer. The user
  completes required native steps, such as dragging an application or using an
  installation wizard. The client automatically re-detects installation and only
  reports success after it discovers the actual installed tool.
- Q5: when no officially compatible installation method exists for the current
  operating system/architecture, retain the tool entry and a disabled Install
  button with the reason. Offer guidance when an official alternative exists.
- Q6: show installation stages and output, support cancellation and failure
  retry, and retain failure details. Cancellation preserves dependencies already
  installed; a GUI installer is cancelled through its own interface.
- Q7: after installation, automatically refresh actual installation and
  connection status. The user separately initiates Connect/Reconnect when
  needed. Installation does not rewrite an existing connection or initiate a
  paid test.
- Q8: actual-tool streaming and tool-use verification is a release acceptance
  requirement. The product's Test action remains a lightweight protocol check
  using the selected model. Its result must state its scope, rather than imply
  that the entire Agent runtime passed acceptance.
- Q11: install a verified compatible official stable version. Reuse an existing
  installation without automatically upgrading it. If its version is
  incompatible, explain the reason and the available resolution. Upgrade and
  uninstall management remain outside this scope.

## Rationale

The earlier uninstalled-tool action only opened an official download page.
The user chose an integrated installation-to-connection flow for every tool,
rather than adding installation only for newly supported products. A normal
text response alone cannot establish that an Agent works through Token Market,
so acceptance also covers streaming and actual tool use.

## Implementation and acceptance checks

The user explicitly confirmed the test boundaries during specification
publication: shared management entry points for installation state, connection
configuration and restoration; desktop interaction checks; and separate native
installation plus actual-tool streaming/tool-use release acceptance.

- Bind each installer to its verified official product, stable version and
  supported operating system/architecture. Do not treat a product name, an
  upstream `main` branch, or an installer exit code as installation proof.
- Verify dependencies, command/application discovery and native installer
  handoff on supported systems, including cancellation and retry.
- Verify each new adapter's actual authentication, supported request protocol,
  model catalogue configuration and configuration restoration. Explain absent
  compatible models instead of writing an unusable connection.
- Keep automated fixtures, native installation checks, paid protocol checks and
  actual-tool streaming/tool-use acceptance as distinct evidence. Do not claim
  installed-tool acceptance from source or mocked tests.
