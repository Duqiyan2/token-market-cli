# Tool installation research

Official documentation checked on 2026-11-02 during the design interview for
[ADR 0005](adr/0005-tool-support-and-installation-scope.md). These are upstream
documentation findings, not installed-tool, target-deployment or paid-call proof.
Resolve the supported upstream version and platform before implementing each
installer. The screenshot's installation captions are not an install manifest.

Scope update: the user has deferred Gemini CLI integration because Token Market
has no supported models for it. Its research below is retained as reference,
not an implementation target. Antigravity CLI is also explicitly deferred. The
active scope is the other twelve tools; neither deferred tool adds a pending
design decision to this round.

## Official installation and compatibility findings

| Tool | Upstream installation reference | Integration constraint |
| --- | --- | --- |
| Claude Code | [Setup](https://code.claude.com/docs/en/setup): native installers for macOS/Linux/Windows; package-manager alternatives | Prefer a supported native installer rather than requiring Node for every tool. |
| Codex CLI | [CLI](https://learn.chatgpt.com/docs/codex/cli), [Windows deployment](https://learn.chatgpt.com/docs/enterprise/windows-deployment): standalone installers and npm | Install for the actual user and re-read effective PATH. |
| WorkBuddy | [macOS](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Installation-Mac-Guide), [Windows](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Installation-Win-Guide): native GUI installers | DMG/EXE installation needs a distinct handoff and detection flow. Official [UOS](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Installation-UOS-Guide) and [Kylin](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Installation-Kylin-Guide) distribution does not establish support for arbitrary Linux distributions. |
| DeepSeek Harness | [Downloads](https://www.deepseek.com/download/), [repository](https://github.com/deepseek-ai/deepseek-harness), [desktop command registration](https://github.com/deepseek-ai/deepseek-harness/blob/master/apps/desktop/README.md) | Desktop installation and registering the `dsh` command are separate operations. Select the intended CLI/desktop variant explicitly. |
| OpenClaw | [Installer](https://docs.openclaw.ai/install/installer), [first run](https://docs.openclaw.ai/start/getting-started) | The official flow can install dependencies and start onboarding. Skip onboarding/model calls when installing; do not silently start billable verification. |
| OpenCode | [Installation](https://opencode.ai/docs/) | Current V1/V2 install channels can resolve to different major versions using the same command name. Match the installer to the adapter's verified configuration format. |
| Gemini CLI | [Installation](https://geminicli.com/docs/get-started/installation/), [configuration](https://geminicli.com/docs/reference/configuration/) | Official npm package `@google/gemini-cli`; custom `GOOGLE_GEMINI_BASE_URL` uses Gemini API, not OpenAI compatibility. [Headless events](https://geminicli.com/docs/cli/headless/) expose streaming and tool use. |
| Grok Build | [Official repository](https://github.com/xai-org/grok-build/blob/main/README.md), [settings](https://docs.x.ai/build/settings/reference) | Current official product uses Rust binaries and scripts rather than the screenshot's npm caption. Custom model entries support multiple protocol backends. |
| Hermes | [Installation](https://hermes-agent.nousresearch.com/docs/getting-started/installation), [platform matrix](https://hermes-agent.nousresearch.com/docs/getting-started/platform-support), [providers](https://hermes-agent.nousresearch.com/docs/integrations/providers) | Dependency bootstrapping is substantial. CLI platform support differs from prebuilt desktop availability; Intel macOS CLI is not categorically unsupported. |
| Pi | [Official README](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/README.md), [models](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md), [Windows](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/windows.md) | Current npm package is `@earendil-works/pi-coding-agent`, requiring Node 22.19+. Existing auth can take precedence over provider configuration; shell availability must also be checked. |
| MiniMax Code | [Official README](https://github.com/MiniMax-AI/minimax-code/blob/main/README.md), [installation](https://github.com/MiniMax-AI/minimax-code/blob/main/docs/installation.md) | Official scripts/npm package `@minimax-ai/code`; custom providers supported. `provider add --use` performs a real model test, so it cannot be copied into a configuration-only Connect flow. |
| Kimi Code | [Current repository](https://github.com/MoonshotAI/kimi-code), [installation](https://github.com/MoonshotAI/kimi-code/blob/main/docs/en/guides/getting-started.md), [providers](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers) | Current upstream is TypeScript/npm `@moonshot-ai/kimi-code`, not the older Python installation. Windows shell prerequisites and tool-use model capabilities need verification. |
| Antigravity CLI | [CLI getting started](https://antigravity.google/docs/getting-started?tab=cli), [BYOK](https://antigravity.google/docs/install?tab=cli) | Official command `agy` is distinct from the IDE. BYOK uses Gemini protocol with a custom base URL and environment credential. |
| Oh My Pi | [Official repository](https://github.com/can1357/oh-my-pi) | Official command `omp`; script/binary or Bun installation. Its provider configuration is `~/.omp/agent/models.yml`, distinct from Pi. Verify supported runtime/binary for the selected platform. |

## Gemini-family model selection findings

These findings come from the current upstream documentation and `main` source.
Both Gemini CLI and Antigravity CLI are deferred; their model-selection proposals
are withdrawn from the current interview. Check these findings against an actual
release if these integrations are revisited.

- Gemini CLI accepts an explicit arbitrary model ID through its
  [model command](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/ui/commands/modelCommand.ts).
  Its ordinary menu is distinct from a complete custom-model catalogue. The
  [model dialog](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/ui/components/ModelDialog.tsx)
  uses custom visible model definitions when experimental dynamic configuration
  is enabled; the
  [schema](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/config/settingsSchema.ts)
  declares that setting. Enabling it is not an already accepted product decision.
- Gemini CLI's
  [startup selection](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/cli/src/config/config.ts)
  prioritizes a command argument, then `GEMINI_MODEL`, then the saved selection,
  then a built-in default. Persistently setting `GEMINI_MODEL` can override the
  user's subsequent saved choices. The official
  [model selection documentation](https://geminicli.com/docs/cli/model/)
  also distinguishes the main model from sub-Agent model choices.
- Antigravity CLI's
  [official settings](https://antigravity.google/docs/settings?tab=cli)
  support a custom model list, labels in the model menu, and a saved selected
  label. Custom entries use Gemini API URLs and the Gemini API-key identity;
  Google Account mode ignores that custom list.
- Importing a catalogue alone does not establish that either tool's first
  request uses a Token Market-compatible model. Both retaining a compatible
  selection and handling a missing/incompatible selection require deliberate
  configuration policy. ADR 0003's no-default-selection rule must be explicitly
  revisited if a new selection step is chosen.
