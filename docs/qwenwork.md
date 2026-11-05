# 千问办公（QwenWorkCN）

工具 ID 为 `qwenwork`。桌面端“一键接入”和 CLI 共用外置 worker 与本地代理方案：

```sh
tokenmarket connect qwenwork
tokenmarket reconnect qwenwork
tokenmarket disconnect qwenwork
```

先安装、登录千问办公，再完全退出后接入。接入会自动准备外置 worker、原生本地代理和模型目录；不需要 Python，不修改 `app.asar`、应用包或厂商签名，不重新打包千问。重新打开客户端后选择 `Token Market · …` 模型。

升级、降级或重新安装千问办公后，完全退出客户端，返回 Token Market 点击“重新接入”。程序按当前安装重新生成外置 worker、本地代理、模型目录和加载配置；托管配置丢失或被改成其他值时直接覆盖，无须先取消接入。保留用户其他模型、聊天和其他设置。重装后尚未登录初始化的客户端，需要先登录。

接入不按应用版本号或单一 SDK 文件指纹限制。程序检查应用是否保留外置 worker 加载接口、数据库结构和 SDK 路由契约；动态识别混淆后的函数名，再用千问自带 Node 执行离线自检。自检只使用虚拟凭据、虚拟模型和本机接口，禁止远程联网，验证策略解析、占位凭据校验、模型组装、流式工具调用及结果回传；不读取真实 API Key，不调用计费模型，不依赖系统安装 Node/Python。

接口结构兼容且自检通过的新 SDK 可直接接入，无需先发布新版 Token Market。接口变化、外置加载机制消失或自检不通过时，统一提示“当前安装的千问办公尚未适配本地接入方案，请联系管理员适配；原连接和凭据已保留”。此时不签发 Key、不改模型和加载配置。不能保证结构发生重大变化的未来 SDK 自动适配。

已核对当前 macOS 1.5.0（26110307）自带 SDK 1.0.53 / worker 1.1.71，以及官方 worker 1.1.60、1.1.69、1.1.70、1.1.71 的发布构建。上述构建通过相同的本机流式工具调用自检。1.1.71 的 Windows x64 worker 脚本与该版本 macOS 脚本指纹相同，也经过脚本自检；这不代表 Windows 原生客户端已完成验收。官方构建来源及指纹见 [兼容性核对记录](qwenwork-sdk-compatibility.json)，不随工具分发原始 SDK。

## 请求与凭据

模型请求经过“千问外置 worker → 本机原生代理 → Token Market 网关”。真实 API Key 只由原生代理附加到发往已配置网关的 Authorization，worker 不持有真实 Key，模型请求不经过千问推理服务。

- 真实 Key 保存在 `~/.tokenmarket/desktop-connections/` 的 AES-256-GCM 加密记录中，主密钥由系统凭据库保管。备份和连接收据也加密。
- 千问的 `byok_custom_models` 只保存 `tokenmarket-local-v2:<SHA256 fingerprint>` 占位符。`plainBase64` 仅包装这个非秘密标记，里面没有真实 Key。
- 模型 URL 使用 `https://tokenmarket.local.invalid/v1/chat/completions` 作为本地路由标记，不向这个域名发请求。
- 外置 worker 使用 SDK 原有传输和 SSE 解码器，保留流式文本、工具调用及取消信号。每个 worker 启动原生代理，从私有标准输出获得随机回环端口与短期访问令牌；令牌只用于本机代理。
- 代理仅监听 `127.0.0.1`，校验访问令牌、Host、请求路径及连接模型目录，拒绝浏览器 Origin，不使用千问进程的 HTTP 代理，不跟随上游重定向。上游错误使用通用文案，正常响应中意外回显的完整 Key 也会过滤。
- 代理每次请求重新核对连接和环境。取消接入或更换 Key 后，旧代理不能继续发起请求；worker 退出后代理停止。

这是应用间的数据流隔离，不是对同一用户权限下恶意本机程序的安全沙箱。千问登录、同步、标题等辅助功能仍可能访问千问服务；本实现不向这些功能提供真实 API Key。历史方案已经发送的数据无法由迁移撤回，曾经写入或发送过的旧 Key 可由用户在平台重置。

## 文件与迁移

- macOS 模型库：`~/Library/Application Support/QwenWorkCN/data/agents.db`。
- Windows 模型库：`%APPDATA%/QwenWorkCN/data/agents.db`。
- 设备配置：`~/.qwenworkcn/.env`，管理 `ALLOW_CUSTOM_BYOK` 和 `QW_QODER_WORKER_RUNTIME_PATH` 两项。
- 外置运行时：`~/.qwenworkcn/tokenmarket-local-sdk-v2/<SDK、代理程序、环境指纹>/`，由客户端复制当前安装 SDK 的必要资源和自身原生代理，不随仓库分发千问 SDK。

只导入 Key 可见且有 `TEXT_GENERATION` 能力的模型。保留用户的其他模型、默认选择、会话及其他数据库表；模型库使用 SQLite 事务和行快照，不用整库覆盖。接入自身不产生推理，主动测试或实际对话可能产生费用。

重新接入会移除旧的 `byok-tokenmarket-` 托管模型，并用占位凭据重建目录。每次从当前安装重新写入生成的运行时文件，可修复外置 worker、原生代理或资源损坏。写入时启用 SQLite `secure_delete`，取消接入不把旧真实凭据恢复回千问数据库。模型目录和设备配置共同受回滚保护；重新接入覆盖托管字段，取消接入仍对接入后的人工修改提示恢复冲突。外置运行时目录保留供已启动进程安全退出，取消接入会恢复加载路径。

macOS 的真实 SDK 策略解析、模型组装、调度、HTTP/SSE、两轮工具调用和结果回传有本机端到端测试；代理还验证了错误令牌、浏览器来源、越权模型、连接撤销与跨流分片 Key 回显过滤。测试使用虚拟 Key，不把真实 Key 交给 SDK：

```sh
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --lib qwenwork_relay
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --test qwenwork
```

Windows 有路径与配置离线测试，尚未完成外置 worker 真机验收。接口文本测试或基本真实对话不代表所有桌面 Agent 工具能力均已验收。语音、图像生成等专项能力不在本次模型替换范围。

2026-11-05 本机迁移后，用原版千问的 `glm-5.3` 实际对话得到 `OK`，SDK 日志记录 `TokenMarketLocal`、回环接口和响应 200；13 个托管模型的数据库凭据均为占位符，官方应用严格签名校验通过。此验收不包含 Windows，也不代表已发布新版 Token Market 安装包。

服务端工具注册需包含 `qwenwork`。实现参考 [zanderzhng/qwenwork-byok](https://github.com/zanderzhng/qwenwork-byok) 的 SDK 直连思路（MIT，commit `09d798d4e8fa42b5a1ff8533f55984f8a7ea90aa`），版权许可见 `scripts/licenses/qwenwork-byok-MIT.txt`。
