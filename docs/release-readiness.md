# 首个版本发布验收

## 验证范围

- 工具安装、接入、重新接入、取消接入和配置恢复分别验证。
- CLI 完成五个平台的打包与安装 smoke；桌面端分别验证 macOS 与 Windows 安装包。
- 自动化夹具、原生安装、操作系统凭据库、浏览器回跳与实际 Agent 运行独立记录。
- 逐工具确认流式对话与工具调用，不将单次文本测试扩大为完整 Agent 验收。

macOS arm64 和 Linux arm64 的有界运行记录见
[macOS 验收](tool-runtime-acceptance-2026-11-03.md)与
[Linux 验收](tool-runtime-linux-acceptance-2026-11-03.md)。DuMate 的验证范围见
[个人版](dumate.md)与[企业版](dumate-enterprise.md)说明。

## 待验证项

- Windows 原生 Agent 运行。
- Intel macOS、其他 Linux 原生目标，以及普通 Linux 宿主机上的 Codex 沙箱行为。
- 逐工具首次选择、完整连接生命周期与凭据持久化。
- 安装包在真实设备上的冷启动、浏览器回跳与重启恢复。
- macOS Apple Developer ID 签名与公证；当前打包使用 ad-hoc 签名。

构建通过不等于设备交互或实际模型请求通过。各项结果应记录实际平台、工具版本和验证范围。
