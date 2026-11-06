# DuMate 企业版模型接入

核实日期：2026-11-03。依据官方 DuMate `1.0.89` / build `447` 安装包，
包括 `app.asar` 中的 Electron/前端代码、`dumate-main-server` 的 Go 符号、
反汇编和反射类型元数据。版本来源见[个人版调查](dumate.md)与
[官方更新动态](https://cloud.baidu.com/doc/Dumate/s/cmmu7shvw)。
本文件记录静态合同，不代表真实企业账号、管理员权限或模型运行验收通过。


## 使用方式

1. 安装官方 DuMate 并登录企业版；接入不要求与安装器版本相同，依据当前活动账号、配置结构及实时企业权限校验，`1.0.89` 是已验收版本；管理员须同时开放自定义模型与模型选择权限。本次支持 HTTPS 企业来源；HTTP 私有部署暂未验证。
2. 完全退出 DuMate，在 Token Market 选择独立的“百度搭子 DuMate（企业版）→ 接入”，或执行 `tokenmarket connect dumate-enterprise`。
3. 接入导入带有效长度元数据的文本模型，初始状态是未测试；不会创建或切换 DuMate 会话，也不会自动调用模型。
4. 保持 DuMate 完全退出，在 Token Market 的工具详情选择一个模型并确认费用测试，或显式执行 `tokenmarket test dumate-enterprise --model <模型 ID>`。这会发送一次真实的非流式文本请求，并将该模型的真实检查结果写回 DuMate。通过后模型为 `normal`，失败为 `abnormal`；其他未测试模型保持原状态。
5. 重新打开 DuMate 企业版，在会话模型列表中选择通过检查的 `Token Market · <模型 ID>`。配置与检查结果落盘，普通重启会保留。
6. 取消接入前同样完全退出 DuMate；`tokenmarket disconnect dumate-enterprise` 只恢复本连接拥有的字段，保留其他模型、重排及未知字段。账户切换或权限撤销后仍可清理原文件。

个人版 `dumate` 与企业版 `dumate-enterprise` 是独立连接和凭证范围，共用官方应用与安装锁。

## 实现与验证

连接前、配置发布前、显式测试前与结果写回前，均使用 DuMate **当前活动登录槽**的 Cookie 查询两项企业权限。历史 profile 中保留的 Cookie 不能在退出登录后被重新启用。Cookie 只在内存解密，并仅发往当前企业来源的固定权限 GET；不会保存到 Token Market、写入日志或发送给 Token Market 服务。不会创建或修复 DuMate 的 `.cookie-key`。

企业模型使用 `personal:tokenmarket-<凭证范围摘要>-<模型配置摘要>` 稳定标识。服务地址、Key 或模型连接元数据改变时，生成新的未测试条目；相同配置重新接入保留上游已完成的真实检查。恢复记录以 `modelKey` 定位，不依赖数组下标。`accessStatus`、检查时间和输出参数探测结果属于派生检查状态，不作为静态配置所有权字段。

为适配下文的官方 UI 缺口，本实现的企业显式测试会发送带 `max_tokens` 的一次真实请求；仅收到无错误的完整助手文本响应后写入 `normal`、真实检查时间、`maxOutputTokensField=max_tokens` 和 `maxOutputTokensFieldSource=probe_accepted`。这项结果来自 Token Market 的实际请求，不表示调用了 DuMate 自身的应用内检查接口，也不代表完整 Agent 工具调用通过。

已覆盖隔离回归：权限关闭/重定向/中途撤销、凭据损坏、账号切换、多个模型导入、确认费用前零调用、单模型检查通过/失败、重排与上游检查结果保存、取消接入恢复。另运行官方原生后端，验证企业文件加载、目录可见、初始未测试，以及模拟真实检查后保存的 `normal` 在重启后仍能读取。见[原生验证记录](acceptance/2026-11-03-dumate-enterprise-runtime.json)。所有请求均使用本机 mock 和虚拟凭据；真实企业权限、真实模型、完整 Agent 与 Windows 原生运行仍未验收。

最终验证：核心 237 passed / 1 ignored，桌面 UI 27 passed。

## 持久文件与隔离

`pref.personalModelsFilePath`（`pref.go:62-63`）返回：

```text
<DuMate userData>/qianfan_desk_xdg/<enterprise userInfo.userId>/config/desktop-main/personal-models.json
```

根结构为 `[]PersonalModel`，不是对象。`pref.savePersonalModels`
（`pref.go:104-134`）将数组编码为 JSON，通过临时文件、同步和重命名发布；
目录权限为 `0700`，文件权限为 `0600`。其中 API Key 为模型记录的一部分，
不能输出到日志或提交真实文件。

Electron `orchestrator/init-services.js:86-95` 在企业版使用
`userInfo.userId`，个人版使用 `bceUserId`。`path/xdg-paths.js` 仅以
`userData`、`qianfan_desk_xdg`、`userId` 构造目录，`tenantId` 单独传递到
运行上下文，未加入这个文件路径。不能自行制造带 tenant 的目录，因为上游
不会按该目录读取。连接记录应额外保留企业域、tenant 与 userId 身份，切换时
重新核对；静态代码不能证明不同企业的 userId 永远不重复。

## PersonalModel JSON

Go `pref.PersonalModel` 的导出字段及 JSON 名称如下；标为可选的字段具有
`omitempty`，其他字段即使是零值也会被上游编码。

| JSON 字段 | 类型 | 说明 |
| --- | --- | --- |
| `modelKey` | string | 本机模型唯一标识 |
| `displayName`、`description` | string，可选 | 展示信息 |
| `modelID` | string | 上游模型 ID |
| `protocol` | string | Validate 只接受 `openai_chat_completions` |
| `baseURL` | string | OpenAI 兼容服务 Base URL |
| `apiKey` | string，可选 | 模型密钥 |
| `maxOutputTokensField` | string，可选 | `max_tokens` 或 `max_completion_tokens` |
| `maxOutputTokensFieldSource` | string，可选 | 探测来源，不能伪造 `probe_accepted` |
| `modelCategory` | string | 模型分类 |
| `supportsImageUnderstanding` | bool | 图像理解能力 |
| `thinkingMode` | string，可选 | 思考模式 |
| `contextTokens`、`outputTokens` | int64 | 模型长度信息 |
| `accessStatus`、`accessStatusMessage` | string，可选 | 连接检查状态与原因 |
| `connectionCheckedAt`、`createdAt`、`sortOrder` | int64，可选 | 检查时间、创建时间、顺序 |

没有 `enabled` 字段。不要加入额外开关来伪装上游已授权或已检查。

个人模型的 `Validate`（`prefs.go:121-149`）不校验分类或思考模式枚举。
上游 `mapManagedModels`（`registry.go:550`）对缺失分类回落为 `LLM`；
`localCustomModels` 保留输入分类。对于 Token Market 文本生成模型，可填写
`modelCategory: "LLM"`。思考模式的上游词汇为 `never`、`always`、
`switchable`、`unknown`（`thinkingModeFor`，`:667-675`）；本地记录缺失
思考模式时回落为 `unknown`（`:636`），不应凭模型名称猜测其他值。

企业个人模型的长度校验只要求 `contextTokens > 0`、`outputTokens > 0`、
`outputTokens <= contextTokens`（`prefs.go:146`），没有发现后端固定最大值。
这与个人版旧单模型表单的 40–2048 K / 1–128 K 范围不同，不能混用两条
配置路径的限制。接入仍应保留模型目录给出的真实长度，并避免制造能力值。

`GeneratePersonalModelKey`（`prefs.go:267-281`）生成
`personal:<slug>-<8 位 UUID 十六进制前缀>`。slug 来源为去空白、转小写后的
展示名称，保留 ASCII a-z/0-9，其他字符折叠为 `-` 并去掉首尾 `-`；
空 slug 回落到 `model`。导入应使用稳定、客户端可识别的键并保留不属于
Token Market 的记录，不能为同一模型在每次重连生成新键。

## 启动、目录与选择

`pref.Startup` 读取 `personal-models.json`，反序列化后规范化记录；
`modelregistry.Registry.Load` 会调用 `reloadLocalCustomModelsLocked`，
`localCustomModels`（`registry.go:617-647`）从偏好构建 `source=personal`、
`routeMode=direct`、`credentialMode=embedded` 的本机模型。派生的
`desktop-main/model-cache.json` 归 DuMate 管理，不能作为连接的持久写入目标。
`openc.StartModelSync` 还会同步模型目录和生成配置；直接改生成的
`opencode/opencode.json` 仍会被上游覆盖。

模型选择是会话级数据。前端 `getModelSelection()` 返回 `{mode:"auto"}` 或
`{mode:"fixed",modelKey}`，随后通过 `sessionUpdate` 保存；企业会话调用
`<enterprise API prefix>/session/update`。没有发现
`personal-models.json` 中存在全局默认选择字段。新会话有内置模型时优先 Auto，
否则选标记默认的模型或首个模型。接入目录不应重写已有会话，也不应承诺新
对话自动使用 Token Market；用户需要在 DuMate 中选择导入的模型。

## 权限与测试边界

企业前端实时查询两项权限：

- `GET <enterprise domain>/api/dumate-enterprise/app/v1/model/config`：
  `appCreateModelEnabled === true` 才允许个人创建模型。
- `GET <enterprise domain>/api/dumate-enterprise/app/v1/modelwhite/config`：
  `switchs.model_controller === true` 才启用模型选择。

前端权限 Store 默认关闭，并通过当前账号的请求获取权限；DesktopModelClient
只在内存中保留约 10 秒的权限结果。本次没有发现可以证明实时管理员授权的
持久权限文件。后端二进制没有上述权限路径或字段的字面量，已检查的偏好加载、
本机模型注册、固定模型选择和直接模型路由也未读取这些权限。因此不能仅靠
离线写入模型文件声称管理员允许；应通过当前已登录 DuMate 的官方请求桥或
界面确认两项权限。不能把历史缓存、模型文件存在或用户属于企业当作放行依据。

官方 Electron `component/session-proxy/console-client.js:173-191` 在这类
请求中使用 Cookie 鉴权，并注入以下请求头：`Content-Type: application/json`、
`X-Requested-With: XMLHttpRequest`、当前企业 Origin、该 Origin 加 `/` 的
Referer、`dumate-version`、`X-Dumate-Client-Type: desktop`；设备 ID 与渠道
存在时另外附带。没有发现这两个 GET 需要 Authorization 或 accessToken。
`utils/cookie.js:123-138` 从目标域匹配的 `bce-user-info` Cookie 值去掉
首尾单/双引号，作为名为 `csrftoken` 的请求头；不是 `X-CSRF-Token`。

`bceConsoleRequest` 只将 HTTP body 解析后包装为 `{statusCode, data}`，
`EnterpriseWebClient` 取其中的 `data`，未再解包 `result`。因此原始 HTTP
成功 JSON 的授权字段为 `{ "appCreateModelEnabled": true }` 与
`{ "switchs": { "model_controller": true } }`，必须严格是布尔 true。
认证请求应限制到当前企业 Origin 和这两个固定 GET，禁止重定向，按 Cookie
域、路径、有效期和 secure 属性过滤；仅从已过滤 Cookie 中提取 CSRF 值。
HTTP 非 200、错误包络、登录重定向或缺失字段均不能放行。

`accessStatus` 仅规范化为 `normal`、`abnormal`、`untested`，空值或未知值
回落到 `untested`。`AddPersonalModel` 创建时也先写 `untested`。
`normalizeFixedModelSelection`（`session.go:399-400`）要求找到模型且其
`accessStatus == "normal"`，否则返回 `MODEL_NOT_AVAILABLE`。未经检查的
导入记录不能冒充正常模型。

官方 `CreatePersonalModel` 在写入并刷新目录后调用 `checkSavedPersonalModel`
（`model.go:94`）；`UpdatePersonalModel` 在连接字段变化时也调用该检查
（`:123`）。所以不能直接把官方保存按钮或创建接口用于无消费的 Connect。
连接配置和显式模型检查须分开：导入保留 `untested`，真实检查由用户明确
触发，只有真实成功证据才能写入上游检查状态。测试还须验证 DuMate 自己读取
并接受该状态，不能把一般协议测试成功等同于整个 Agent 已通过验收。

### 当前版本 UI 的检查状态持久化缺口

静态 UI 路径为「设置 → 模型 → 对应自定义模型的编辑按钮 → 联通性 → 测试」，
也可以点击编辑框的「确定」先执行检查。但 **1.0.89 的这条路径不能保证将
已经导入的 `untested` 模型变成持久化的 `normal`**，不能把它作为无条件的
导入后使用指引。证据均来自官方安装包：

- `frontend/static/js/index.b36ba629.js` 的编辑表单将 `testModel` 返回值
  仅保存到 React 状态 `N(result)` 以显示「通过」；提交仍为
  `{...initialModel,...formValues}`，没有合并检查返回的状态或时间。
- `DesktopModelClient.checkPersonalModel` 最终仅向
  `global/model/personal/check` 发送 `modelID/protocol/baseURL/apiKey`，
  丢弃编辑表单传来的 `modelKey`。
- `handler.CheckPersonalModel`（`model.go:205-221`）只有在请求提供非空
  `modelKey`、模型存在且四个连接字段与保存记录一致时，才调用
  `checkAndPersistPersonalModelConnection`。没有 key 的请求只调用
  `checkPersonalModelConnection`，不落盘。
- `handler.UpdatePersonalModel`（`:114,:122-123`）仅在四个连接字段发生
  变化时调用 `checkSavedPersonalModel`；没有「状态是 untested 也检查」分支。
  `pref.UpdatePersonalModel`（`prefs.go:380-385`）在连接未变时从旧记录保留
  `accessStatus/accessStatusMessage/connectionCheckedAt` 及探测出的输出字段，
  不会接受表单传入的检查结果。仅改名称、长度或能力也不能触发该检查。

真正更改 `modelID/protocol/baseURL/apiKey` 之一后保存，后端会检查并持久化；
重新添加也会走创建后的检查。这些操作会改变连接或模型身份，不能建议用户
为了激活导入模型随意修改。尤其 `ValidateConnectionConfig` 会去空白并去掉
Base URL 尾部 `/`，添加空白或尾斜杠不能可靠触发变化。

后端已有可持久化的正式检查路径：带原 `modelKey` 和相同四个连接字段请求
`personal/check`，成功后 `persistPersonalModelCheckResult`
（`model.go:368-387`）调用偏好更新并 `ReloadLocalCustomModels`。该本机接口
受 DuMate 的应用内鉴权保护；静态确认存在不等于外部客户端已取得调用权限，
也不等于已经运行验收。接入方必须实现有真实成功证据的显式检查流程，或在
官方修复 UI 后再承诺仅通过上述界面启用导入模型。

官方 `probePersonalModelField`（`personal_model_probe.go:329-336`）使用
一个简短的 `1+1` 用户消息、`stream: false` 及候选输出上限字段，未提供
`tools` 或 `tool_choice`；`probePersonalModelConnection` 先尝试 `max_tokens`，
再尝试 `max_completion_tokens`。这项检查不要求实际工具调用，只证明连接和
响应协议。流式响应与工具调用仍属于独立的 Agent 验收。

本次未读取真实登录凭据、修改真实 DuMate 配置或发起付费模型请求。
