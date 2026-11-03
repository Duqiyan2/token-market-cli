# Tool support includes installation and runtime verification

Support for a Tool includes installation through a verified official method,
installation detection, model configuration, explicit connection testing and
configuration restoration. A download link or installer exit code alone does
not prove that a tool is installed and usable.

## Installation behavior

- Reuse compatible installations without automatically upgrading them. Prepare
  missing dependencies in a user-level or tool-private directory when possible.
- For graphical installers, open the official installer and let the user complete
  its native steps. Refresh detection and report success only after finding the
  actual executable or application.
- Keep unsupported tool entries visible with a disabled Install button and a
  concrete compatibility explanation.
- Show stages and output, support cancellation and retry, and retain failure
  details. Cancel graphical installers through their own interface.
- Installation does not connect a tool or send a paid request. Connect remains a
  separate user action.
- Use verified compatible official versions. DeepSeek Harness uses the fixed
  official `0.2.1-rc.5` candidate; it is not represented as a stable release.

## Model configuration and verification

Claude Code and Codex use the client's designated default model. Other adapters
import their compatible text-generation catalogue while preserving existing
preferences, with DuMate Personal limited to one model.

Verify actual product identity, configuration discovery, authentication format,
model selection and restoration for each adapter. Keep automated fixtures, native
installation checks, explicit model tests and actual Agent streaming/tool-use
checks as separate evidence. A basic text response does not establish a complete
Agent tool-use loop. Supported tool IDs are listed in the [README](../../README.md).
