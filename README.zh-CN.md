# Token Market

[English](README.md) | [简体中文](README.zh-CN.md)

通过桌面应用或命令行，将本机 AI Agent 工具接入 Token Market，统一管理工具安装、模型配置和连接状态。

[下载桌面端](https://github.com/FitBBC/token-market-cli/releases/latest) · [获取 CLI](https://www.npmjs.com/package/tokenmarket) · [反馈问题](https://github.com/FitBBC/token-market-cli/issues)

## 安装

开始前，请准备好 Token Market 账号。桌面端和 CLI 可以单独使用，也可以在同一台机器上配合使用，共用登录和当前空间。

### 桌面端

从 [Releases](https://github.com/FitBBC/token-market-cli/releases/latest) 下载适合系统的安装包：

| 系统 | 安装方式 |
| --- | --- |
| macOS · Apple Silicon | 打开 `.dmg`，将 **Token Market** 拖入 **Applications**。 |
| Windows · x64 | 运行 `.exe` 或 `.msi` 安装程序。 |

### CLI

需要 **Node.js 20+**。支持 macOS arm64/x64、Windows x64，以及 glibc 2.35+ 的 Linux x64/arm64。

```bash
npm install -g tokenmarket
tokenmarket --version
```

也可以直接运行，无需全局安装：

```bash
npx --yes tokenmarket login
```

## 快速开始

### 使用桌面端

1. 打开 Token Market，点击 **使用浏览器登录**。
2. 在浏览器完成登录，点击 **Open Application** 返回应用。
3. 确认当前空间，新连接使用这个空间计费。
4. 选择工具；尚未安装时，点击 **安装** 并按提示完成操作。
5. 安装完成后点击 **接入**。
6. 重新打开工具，按提示选择 Token Market 模型。

### 使用命令行

以 Claude Code 为例：

```bash
tokenmarket login
tokenmarket workspace
tokenmarket install claude-code
tokenmarket connect claude-code
tokenmarket status claude-code
```

已安装工具时，可以跳过 `install`。接入后重新打开工具；终端工具也可以通过以下命令启动：

```bash
tokenmarket run claude-code
```

**安装、接入和重新接入不会自动发送模型请求。** 需要检查模型连接时，可在桌面端点击 **测试**，或运行：

```bash
tokenmarket test claude-code
```

测试会发送一次可能产生费用的模型请求，执行前会要求确认。

## 支持的工具

桌面端直接选择工具，CLI 命令使用对应的工具 ID。可用工具和安装方式以所用客户端的显示为准。

| 工具 | CLI 工具 ID |
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
| 百度搭子 DuMate 个人版 | `dumate` |
| 百度搭子 DuMate 企业版 | `dumate-enterprise` |
| [千问办公](docs/qwenwork.md) | `qwenwork` |

### 接入提示

- **模型选择**：Claude Code 和 Codex 使用客户端指定的默认模型；DuMate 个人版一次配置一个模型。其他工具导入兼容的文本生成模型，接入后按工具提示选择。
- **Grok Build、Pi、Oh My Pi**：接入后通过 `/model` 选择 Token Market 模型。Pi 已保存 `tokenmarket` 凭据时，请先在 Pi 中退出该 provider，再接入。
- **DuMate 个人版**：先登录 DuMate，完全退出应用后再接入或取消接入，完成后重新打开。
- **千问办公**：先登录并完全退出应用后接入或取消接入；重新打开后选择 Token Market 模型。更新千问办公版本后，需要返回 Token Market 重新接入。
- **DuMate 企业版**：管理员须开放自定义模型和模型选择权限。完全退出 DuMate 后接入，保持退出状态，在 Token Market 中选择模型并确认测试；重新打开 DuMate，在会话中选择通过检查的模型。

## 管理连接

| 操作 | 桌面端 | CLI |
| --- | --- | --- |
| 安装工具 | **安装** | `tokenmarket install <tool>` |
| 启动终端工具 | **打开工具** | `tokenmarket run <tool>` |
| 查看安装与连接状态 | **重新检测**或打开工具详情 | `tokenmarket status [tool]` |
| 更新连接 | **重新接入** | `tokenmarket reconnect <tool>` |
| 恢复接入前的配置 | **取消接入** | `tokenmarket disconnect <tool>` |
| 测试指定模型 | **测试**并选择模型 | `tokenmarket test <tool> --model <id>` |
| 退出登录 | 账号菜单 → **退出登录** | `tokenmarket logout` |

将 `<tool>` 替换为上表中的工具 ID。`status` 不带工具 ID 时会列出全部工具；工具配置了多个模型时，测试需通过 `--model <id>` 指定模型。

安装与接入是两个独立步骤。已有兼容安装会直接复用；安装中断后，可以再次点击 **安装** 或重试命令。图形安装器需在其自身界面取消，**停止等待** 只停止 Token Market 对安装进度的监控。

### 切换空间

先查看可用空间，再使用空间 ID 切换：

```bash
tokenmarket workspace
tokenmarket workspace <workspace-id>
```

切换当前空间会影响新连接，已有工具仍使用接入时绑定的计费空间。需要更换已有工具的计费空间时，切换后执行 `tokenmarket reconnect <tool>`。账号或计费空间变化时会要求确认。

### 取消接入与退出登录

取消接入会恢复本次连接管理的配置字段。遇到接入后的手动修改时，会询问保留当前值还是恢复原值。退出登录会保留工具配置和保存的凭据；需要恢复工具原配置时，请使用 **取消接入**。

### SSH 与非交互使用

在没有浏览器的机器上运行：

```bash
tokenmarket login --no-browser
```

在其他设备打开输出的授权链接并完成登录，终端会等待确认。

非交互执行时，按需明确指定以下选项：

| 场景 | 选项 |
| --- | --- |
| 确认重新接入时更换账号或空间 | `--confirm-rebind` |
| 本地凭据副本全部丢失后恢复连接 | `--confirm-recovery` |
| 取消接入时保留手动修改 | `--conflicts keep-current` |
| 取消接入时恢复原值 | `--conflicts restore-original` |

运行 `tokenmarket --help` 或 `tokenmarket <command> --help` 查看命令说明。

## 常见问题

| 问题 | 处理方式 |
| --- | --- |
| 找不到已安装的工具 | 点击 **重新检测**；使用 CLI 时确认工具可从终端启动，必要时重新打开终端。 |
| 接入后仍使用旧配置 | 完全退出并重开工具；OpenClaw 还需重启会话，OpenCode 需检查项目配置是否覆盖全局设置。 |
| 显示“配置已变化”或无法测试 | 按提示修复配置或重新接入，再执行测试。 |
| npm 提示缺少内置二进制 | 运行 `npm install -g tokenmarket` 重新安装。 |
| 凭据失效或无法读取 | 按客户端提示重新登录或到 Token Market 检查凭据状态；系统凭据库锁定时先解锁。 |
| 需要恢复原配置 | 点击 **取消接入**，或运行 `tokenmarket disconnect <tool>`。 |

### macOS 首次打开被拦截

安装包采用本地签名，尚未使用 Apple Developer ID 签名和公证。确认安装包来自本仓库 Releases 后，可运行：

```bash
xattr -cr "/Applications/Token Market.app"
```

然后重新打开应用。

macOS 使用系统钥匙串工具保持 App、CLI 和升级后的凭证访问者稳定，自签名构建同样适用。
旧原生钥匙串条目迁移时可能需要一次授权或显式重置。构建与发布配置见
[macOS 签名说明](docs/macos-local-signing.md)。

## 问题反馈

请通过 [GitHub Issues](https://github.com/FitBBC/token-market-cli/issues) 反馈问题，附上操作系统、Token Market 版本、工具名称和复现步骤。提交截图或日志前，请移除 API Key、登录凭据等敏感信息。
