# Token Market

[English](README.md) | [简体中文](README.zh-CN.md)

Connect local AI agent tools to Token Market from the desktop app or command line. Manage tool installation, model configuration and connection status in one place.

[Download desktop app](https://github.com/FitBBC/token-market-cli/releases/latest) · [Get the CLI](https://www.npmjs.com/package/tokenmarket) · [Report an issue](https://github.com/FitBBC/token-market-cli/issues)

## Installation

You need a Token Market account. Use the desktop app and CLI independently or together on the same machine; both share your sign-in and current workspace.

### Desktop app

Download the installer for your system from [Releases](https://github.com/FitBBC/token-market-cli/releases/latest):

| System | Installation |
| --- | --- |
| macOS · Apple Silicon | Open the `.dmg` and drag **Token Market** into **Applications**. |
| Windows · x64 | Run the `.exe` or `.msi` installer. |

### CLI

Requires **Node.js 20+**. Supports macOS arm64/x64, Windows x64 and Linux x64/arm64 with glibc 2.35+.

```bash
npm install -g tokenmarket
tokenmarket --version
```

Or run without a global installation:

```bash
npx --yes tokenmarket login
```

## Quick start

### In the desktop app

1. Open Token Market and click **使用浏览器登录** (Sign in with browser).
2. Complete sign-in in your browser, then click **Open Application** to return to the app.
3. Confirm the current workspace. New connections use this workspace for billing.
4. Choose a tool. If it is missing, click **安装** (Install) and follow the prompts.
5. After installation, click **接入** (Connect).
6. Reopen the tool and select the Token Market model as prompted.

### In the terminal

For example, to connect Claude Code:

```bash
tokenmarket login
tokenmarket workspace
tokenmarket install claude-code
tokenmarket connect claude-code
tokenmarket status claude-code
```

Skip `install` if the tool is already installed. Reopen the tool after connecting. You can also launch terminal tools with:

```bash
tokenmarket run claude-code
```

**Installation, connection and reconnection do not automatically send model requests.** To check a model connection, click **测试** (Test) in the app or run:

```bash
tokenmarket test claude-code
```

Testing sends one potentially billable model request and asks for confirmation before proceeding.

## Supported tools

Select a tool in the app, or use its ID in CLI commands. Available tools and installation methods depend on the client you are using.

| Tool | CLI ID |
| --- | --- |
| Claude Code | `claude-code` |
| Codex CLI | `codex` |
| WorkBuddy | `workbuddy` |
| DeepSeek Harness | `deepseek-harness` |
| OpenClaw | `openclaw` |
| OpenCode | `opencode` |
| Grok Build | `grok-build` |
| Hermes | `hermes` |
| Pi | `pi` |
| MiniMax Code | `minimax-code` |
| Oh My Pi | `oh-my-pi` |
| Kimi Code | `kimi-code` |
| Baidu DuMate Personal | `dumate` |
| Baidu DuMate Enterprise | `dumate-enterprise` |
| QwenWorkCN | `qwenwork` |

### Connection tips

- **Model selection**: Claude Code and Codex use the client's designated default model. DuMate Personal configures one model at a time. Other tools import compatible text-generation models; follow the tool's prompts to select one after connecting.
- **Grok Build, Pi and Oh My Pi**: Use `/model` to select a Token Market model after connecting. If Pi already has a saved `tokenmarket` credential, log out of that provider in Pi before connecting.
- **DuMate Personal**: Sign in to DuMate first, then fully quit it before connecting or disconnecting. Reopen it when done.
- **QwenWorkCN**: Install from its official website and sign in first. Fully quit before connecting or disconnecting, then reopen and select a Token Market model. Reconnect in Token Market after updating QwenWorkCN. See [configuration and limitations](docs/qwenwork.md).
- **DuMate Enterprise**: An administrator must enable custom models and model selection. Fully quit DuMate before connecting. Keep it closed while selecting and confirming a model test in Token Market, then reopen DuMate and select a passing model in the conversation.

## Manage connections

| Action | Desktop | CLI |
| --- | --- | --- |
| Install a tool | **安装** (Install) | `tokenmarket install <tool>` |
| Launch a terminal tool | **打开工具** (Open Tool) | `tokenmarket run <tool>` |
| View installation and connection status | **重新检测** (Refresh) or tool details | `tokenmarket status [tool]` |
| Update a connection | **重新接入** (Reconnect) | `tokenmarket reconnect <tool>` |
| Restore configuration from before connection | **取消接入** (Disconnect) | `tokenmarket disconnect <tool>` |
| Test a selected model | **测试** (Test), then choose a model | `tokenmarket test <tool> --model <id>` |
| Sign out | Account menu → **退出登录** (Sign out) | `tokenmarket logout` |

Replace `<tool>` with an ID from the table above. Omit the tool ID from `status` to list all tools. For tools with several configured models, specify the model to test with `--model <id>`.

Installation and connection are separate steps. Existing compatible installations are reused. If installation is interrupted, click **安装** (Install) again or retry the command. Cancel graphical installers in their own window; **停止等待** (Stop waiting) only stops Token Market from monitoring progress.

### Switch workspaces

List available workspaces, then switch using a workspace ID:

```bash
tokenmarket workspace
tokenmarket workspace <workspace-id>
```

Switching the current workspace affects new connections. Existing tools keep their bound billing workspace. To change an existing tool's billing workspace, switch first, then run `tokenmarket reconnect <tool>`. Account or billing workspace changes require confirmation.

### Disconnect and sign out

Disconnecting restores the configuration fields managed by the connection. If you edited them after connecting, you can choose to keep your changes or restore the original values. Signing out preserves tool configurations and saved credentials. Use **取消接入** (Disconnect) to restore a tool's original configuration.

### SSH and noninteractive use

On a machine without a browser, run:

```bash
tokenmarket login --no-browser
```

Open the displayed authorization URL on another device and complete sign-in. The terminal waits for confirmation.

For noninteractive commands, supply explicit options when needed:

| Situation | Option |
| --- | --- |
| Confirm an account or workspace change during reconnection | `--confirm-rebind` |
| Recover a connection after all local credential copies are lost | `--confirm-recovery` |
| Keep manual edits during disconnection | `--conflicts keep-current` |
| Restore original values during disconnection | `--conflicts restore-original` |

Run `tokenmarket --help` or `tokenmarket <command> --help` for command details.

## Troubleshooting

| Symptom | What to do |
| --- | --- |
| An installed tool is missing | Click **重新检测** (Refresh). For CLI use, check that the tool runs from your terminal and reopen the terminal if needed. |
| A tool still uses old settings | Fully quit and reopen it. Also restart the OpenClaw session; check OpenCode project settings for overrides. |
| Configuration is marked as changed or testing is unavailable | Follow the prompts to repair or reconnect, then test again. |
| npm reports a missing bundled binary | Reinstall with `npm install -g tokenmarket`. |
| Credentials are invalid or unreadable | Follow the client prompts to sign in again or check credential status in Token Market. Unlock the system credential store if needed. |
| You need to restore original settings | Click **取消接入** (Disconnect) or run `tokenmarket disconnect <tool>`. |

### macOS blocks the first launch

Builds signed with the fixed local certificate are not Developer ID signed or notarized. After verifying the source of such an installer, run:

```bash
xattr -cr "/Applications/Token Market.app"
```

Then reopen the app.

Normal upgrades preserve the signing identity so the app and CLI can reuse their
Keychain items without repeated access prompts. Older ad-hoc builds may require
a one-time authorization when migrating. See [macOS signing](docs/macos-local-signing.md)
for build and release configuration.

## Feedback

Report problems through [GitHub Issues](https://github.com/FitBBC/token-market-cli/issues). Include your operating system, Token Market version, tool name and steps to reproduce. Remove API keys, sign-in credentials and other sensitive information before sharing screenshots or logs.
