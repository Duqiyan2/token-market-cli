# Agent 会话 ID

会话标识用于网关日志关联与已有的粘性路由。同一个真实会话的多轮请求保留原 ID，新会话使用其自身 ID，恢复历史会话沿用原 ID；不使用工具连接 ID、设备 ID、API Key 或每次请求新生成的随机值替代。

## 工具接入

| 工具 | 会话来源 | 传递方式 |
| --- | --- | --- |
| WorkBuddy | 原生请求构建器读取当前 session.id | 原生 `X-Conversation-ID`，网关兼容读取 |
| 百度搭子个人版 | 引擎每次模型请求的 `X-Dumate-Session-Id` | 统一接管扩展原样保留原生 Header，网关兼容读取 |
| 千问办公主对话、SDK 子任务 | SDK 请求的 `session-id` | 本地原生代理原样保留原生 Header，网关兼容读取 |
| 千问办公桌面辅助请求 | 官方会话凭据签发请求中的 `session_id` | 返回会话专属的本机 base URL；代理从每次请求路径恢复原 ID，写入 `X-Session-Id` |

搭子子任务保留自己的原生会话 ID；本次没有新增父子关系日志字段。当前搭子的部分原生路由分类请求没有提供会话 ID，这些请求保持缺失，不从其他聊天借用标识。千问未提供会话 ID 的默认辅助凭据范围也保持未关联。搭子企业版使用独立接入路径，本次未增加企业版的客户端注入。

千问的会话路径在回环地址内使用 URL-safe Base64 编码，不作为凭据。随机本机访问令牌、模型目录检查与真实平台 Key 的隔离沿用现有机制；会话专属地址随各会话凭据缓存，多个聊天不会共享一个可变的“当前会话”。代理向网关保留会话 Header，辅助通道按会话路径补充 `X-Session-Id`；厂商登录令牌、Cookie 和其他任意请求头不转发。

## 网关读取

网关仓库 `llm-gateway-router` 的四个既有会话端点统一读取：Chat Completions、Responses、Claude Messages、Gemini Generate Content。按以下顺序选择首个非空标识：

1. 请求体顶层 `session_id`。
2. `extra_body.session_id`。
3. `X-Session-Id`。
4. `x-claude-code-session-id`。
5. `thread-id`。
6. `session-id`。
7. `X-Conversation-ID`。
8. `X-Dumate-Session-Id`。

Header 名称大小写不敏感。Codex 同时提供两个原生字段时，以 `thread-id` 作为聊天身份。网关沿用最多 256 个字符的校验、原值日志关联和上游租户隔离哈希。该逻辑不改变非会话端点的行为。

## 生效与验证

需要部署包含原生 Header 兼容的网关。搭子、千问办公还需要使用更新后的 Token Market 重新接入并完整重启对应工具，以更新已加载的扩展、代理和凭据缓存。仅保存源代码或配置不代表已运行实例生效。

验证命令：

```sh
node --test tests/dumate-unified.test.cjs tests/workbuddy-session-headers.test.cjs
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --lib qwenwork
python3 tests/helpers/dumate-unified-native-probe.py --runtime 0
python3 tests/helpers/dumate-unified-native-probe.py --runtime 1
```

WorkBuddy 测试执行当前安装包中的原生会话 Header 构建片段。千问测试调用真实 SDK 和桌面辅助请求方法，通过本机代理抵达虚拟上游。搭子使用隔离账号目录、原生后端与引擎验证新旧会话、恢复、多轮、子任务和摘要。测试使用虚拟凭据和本机服务，不调用付费模型，不修改真实账号配置；不代表 Windows 原生设备或生产网关已完成验收。
