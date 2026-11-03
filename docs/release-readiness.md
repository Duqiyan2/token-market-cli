# 1.0.0 发布验收与后续工作

维护者要求将当前 main 重新发布为 1.0.0，并清理旧 GitHub Release、发布标签、npm 版本及 issues。历史 issues 正文和评论、旧安装包、npm tarball/元数据及 Git refs 已先归档到本地 `dist/release-reset-20261103/`。旧 issue 编号在历史文档中保留作为当时的记录，不代表仍有在线工单；不新增替代 issues。

## 已有证据

- 十二工具集成实现已合并；main 后续加入的 DuMate 个人版、企业版和环境隔离修复一并保留。
- 原十二工具的 macOS arm64 真实 Agent 流式和读取验收为 12/12；Ubuntu arm64 容器中 11 款 CLI 安装成功，10 款运行通过，Codex 因容器 namespace 环境受阻，WorkBuddy 普通 Ubuntu 不支持。见两份 `tool-runtime-acceptance` / `tool-runtime-linux-acceptance` 报告。
- DuMate 个人版、企业版的有界运行证据和配置边界见各自集成文档；上述十二工具结果不自动扩大为十四款全平台验收。
- 发布流程须完成五平台 CLI 打包与 smoke，以及 macOS / Windows 桌面安装包构建；构建成功与真实设备交互验收分开记录。

## 保留待办

- Windows 原生真实 Agent 验收：用户确认暂无机器，继续延期。
- Intel macOS、其他 Linux 原生目标和普通 Linux 宿主机上的 Codex 沙箱运行。
- 逐工具完整 Connect / Reconnect / Disconnect / restore、空 HOME 首次选择和凭证持久化的原生验收。
- 平台新增工具注册代码的目标部署和线上新工具接入契约验收；复用既有 Key 的 Agent 运行不证明新 Key 颁发。
- macOS Developer ID 签名与公证尚未配置，当前正式环境包为 ad-hoc 签名。

清理 issues 只重置在线工单，不将这些待办标记为完成。1.0.0 按维护者要求重置版本编号；GitHub / npm 均以新版本及其实际构建校验结果为准。
