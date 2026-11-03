# 百度搭子 DuMate 个人版接入

核实日期：2026-11-03。目标是让 DuMate 使用 Token Market 的模型。
已实现桌面端和 CLI 共享接入，目标为官方 `1.0.89` 个人版；企业版另行适配。
下文分别记录使用方式、实现依据和验证范围。使用前请确认客户端提供对应工具入口。

## 使用

1. 安装 DuMate `1.0.89` 并登录个人版；已有安装不会自动升级。
2. 完全退出 DuMate，避免进程缓存覆盖配置。
3. 在源码构建的 Token Market 桌面端选择“百度搭子 DuMate（个人版）→ 接入”，
   或运行源码构建的 `tokenmarket connect dumate`。
4. 重新打开 DuMate；需要时单独发起 `tokenmarket test dumate` 或桌面端测试。
5. 取消接入前同样完全退出 DuMate，运行 `tokenmarket disconnect dumate`，
   再重新打开。发生手动修改时使用共享冲突处理流程选择保留或恢复。

个人版一次配置一个模型，接入或重新接入时优先使用客户端默认模型；
默认模型不可用时，保留目录中仍可用且
能力元数据齐全的原模型，再选择目录中首个有效文本模型。
更换服务地址不会自动改写工具配置；需确认重新接入后才更新配置。
上下文与输出限制来自 `context_window_tokens`、`max_output_tokens`，
输出上限不超过上下文减一；没有有效元数据时保留原配置并报错。
接入与安装不会自动调用模型或修改 DuMate 白名单、登录信息。

配置定位使用 `auth.json` 的活动 `bce` profile 和 `bceUserId`；不扫描历史账号目录。
切换 DuMate 账号后原连接会显示配置改变，原记录仍能恢复原账号文件。
macOS 应用标识必须为 `com.baidu.qianfan.desktop`，单独的 `dumate` 命令不算已安装。

## 官方版本与安装

[官网](https://www.dumate.cn/)提供 macOS Apple Silicon 和 Windows 客户端。
[个人版更新动态](https://cloud.baidu.com/doc/Dumate/s/cmmu7shvw)
列出的最新版本为 `1.0.89`（2026-10-31）。官网脚本指向的
[macOS 安装包](https://dumate-app-public.cdn.bcebos.com/release-info/artifacts/Dumate-mac-arm64-latest.dmg)
于核实日下载后，`Info.plist` 声明版本 `1.0.89`，应用标识
`com.baidu.qianfan.desktop`，包内 `package.json` 声明 build `447`。
包内记录的 main 修订为 `e35aadb162c15ad1c7c449d4754a11b1dd4e3a49`，
frontend 修订为 `1779dc35b0de2ed52d6610aa9b0000fab0d81b03`。
这是可变的 latest 地址，不能直接作为固定版本安装合同。

安装器使用包内指定的官方更新 feed
[`latest-mac.yml`](https://dumate-app-public.cdn.bcebos.com/release-info/artifacts-update/latest-mac.yml)、
[`latest.yml`](https://dumate-app-public.cdn.bcebos.com/release-info/artifacts-update/latest.yml)
中的 `1.0.89` 固定文件与 SHA512。macOS 固定 DMG 地址返回 404，使用已核实可用的
[`DuMate-1.0.89-arm64.zip`](https://dumate-app-public.cdn.bcebos.com/release-info/artifacts-update/DuMate-1.0.89-arm64.zip)：
校验后解压并打开目录，由用户将应用拖入 Applications。Windows 使用
[`DuMate-Setup-1.0.89-x64.exe`](https://dumate-app-public.cdn.bcebos.com/release-info/artifacts-update/DuMate-Setup-1.0.89-x64.exe)，
校验后交给原生向导。安装成功须重新发现实际应用。
支持 macOS Apple Silicon（14+）与 Windows x64（10 1809+）；其他系统保留入口并解释限制。

## 自定义模型的客户端证据

官方 `1.0.89` 安装包的 `Contents/Resources/app.asar` 中存在两条路径。
它们尚未形成稳定公开文档；本次通过包内 JavaScript、Go 符号、反汇编及 Go
反射类型元数据交叉核实个人版的持久化格式。

| 路径 | 静态代码中的合同 | 可用性条件 |
| --- | --- | --- |
| 单模型配置 | `frontend/static/js/async/8221.e22d99b3.js` 模块 `93736` 调用本机 `global/setting/gateway/custom-llm-provider/get`、`set`、`remove`，字段为 `model`、`baseURL`、`apiKey`、`contextLimit`、`outputLimit`。保存前调用 `global/setting/gateway/check-llm-provider`，保存后提示重启。 | UI 查询 `/api/ai_apaas/console/dumate/user/whitelist` 的 `modelConfigEnabled`，默认关闭。 |
| 企业版多模型 | `frontend/static/js/index.b36ba629.js` 的 DesktopModelClient 调用本机 `global/model/list`、`personal/list`、`personal/check`、`personal/create`、`personal/update`、`personal/delete`、`personal/reorder`；检查接口携带 `modelID`、`protocol`、`baseURL`、`apiKey`。 | 企业版模型选择受权限与白名单控制，远端 `modelwhite/config` 的 `switchs.model_controller` 控制模型选择，`appCreateModelEnabled` 控制创建。`personal` 表示模型来源，不能据此认定个人版也提供该功能。 |

单模型表单要求上下文为 40–2048 K tokens，输出为 1–128 K tokens 且小于上下文。
保存按钮会执行连接检查；安装或仅写配置的流程不能照搬该按钮而隐式触发模型消费。
企业版多模型路径不属于此次个人版试用范围。

## 个人版持久化合同

用户已选择先接入个人版。`pref.prefFilePath`（`pref.go:58-59`）将偏好定位为：

```text
<DuMate userData>/qianfan_desk_xdg/<DuMate userId>/config/desktop-main/dumate-app.json
```

Electron 的 `build-ts-electron/path/xdg-paths.js` 生成用户级 `XDG_CONFIG_HOME`；
Go `cfg.Cfg.XDGConfigDir` 读取该环境值，`prefFilePath` 再拼接
`desktop-main/dumate-app.json`。应定位当前 DuMate 用户目录，不能把配置写到
无账号目录或全局目录。Go 反射元数据给出的 JSON 形状为：

```json
{
  "customLLMProvider": {
    "baseURL": "https://api.example.com/v1",
    "apiKey": "<Token Market key>",
    "model": "<selected model id>",
    "contextLimit": 128000,
    "outputLimit": 32000
  }
}
```

示例长度仅说明单位为 tokens，实际值须来自所选模型的能力。
`customLLMProvider` 是一个对象，个人版一次配置一个模型；同文件中还有其他
偏好字段，连接与恢复只能管理该对象，不应替换整个文件。

`pref.Startup`（`pref.go:167-168`）读取此文件并反序列化到内存；
`openc.initOpencodeCfg`（`opencode_cfg.go:159-163`）取出该对象，调用
`writePersonalCustomModel` 生成运行配置。该运行配置把自定义模型与限制写入
`provider.QianfanPersonalQuota.options.personalCustomModel`，保留本机代理地址。
持久偏好因此可在重启后重新生效。

安装包内 `config/opencode/opencode.json` 为个人提供方选用
`@ai-sdk/openai-compatible`。Go `resolveGatewayLLMRoute`
（`gateway_llm.go:624-625`）在个人版取出 `customLLMProvider` 后进入
`legacy_custom` 路由；`customProviderRoute`（`:723-730`）从请求路径去掉
`/v1` 和开头斜线，再拼接所配置的 Base URL，并使用所配置的模型与 API Key。
所以本次协议为 OpenAI Chat Completions，模型 API Base URL 应包含 `/v1`。

`modelConfigEnabled` 的已知用途是控制官方界面的配置入口。本次检查的
`SetGatewayCustomLLMProvider`、`SavePersonalCustomModel`、
`writePersonalCustomModel`、启动读取及 `legacy_custom` 运行路由中，没有发现
再次读取该白名单或检查该字段；其分支区分个人、企业及私有部署登录模式。
这是 `1.0.89` 的静态实现证据，不是对未来版本或实际账号运行成功的承诺。

后端保存入口与 `check-llm-provider` 分离，配置保存路径不调用模型探测。
直接管理偏好时也不需要调用官方 UI 保存按钮。由于进程缓存偏好，外部写入
需在 DuMate 完全退出后进行，并在启动后生效；运行中修改可能被缓存覆盖。

包内 `build-ts-electron/services/opencode/opencode-config.js` 的
`prepareConfigDir` 每次从安装资源读取模板，替换端口、设置技能目录后，直接
`writeFileSync` 写入用户 XDG 配置目录中的 `opencode/opencode.json`；
`orchestrator/init-services.js` 在服务启动阶段调用该函数。因此直接改生成的
`opencode.json` 会被后续启动覆盖，不能作为可持久接入方案。

## 公开文档的边界

[用户设置](https://cloud.baidu.com/doc/Dumate/s/xmquqh8mx)列出通用、资源点、
记忆、消息渠道、浏览器、安全等配置，没有给出外部模型的 Base URL/API Key
配置步骤。文档的缺失不能推导出客户端完全不支持自定义模型。

[标准化 OpenAPI 指南](https://cloud.baidu.com/doc/Dumate/s/ymu8cfo0x)
用于其他应用调用 DuMate 的会话、流式事件和文件能力，要求申请百度侧 Key。
其中 `model.providerID`、`model.modelID` 是模型选择字段，没有公开外部供应商
Base URL/API Key 的注册合同。这条 API 与本任务要求的模型调用方向不同。

[创建智能体](https://cloud.baidu.com/doc/Dumate/s/Gmtim7rk4)和
[更新智能体配置](https://cloud.baidu.com/doc/Dumate/s/Imtim8eog)的请求参数提供
提示词、技能、文件、MCP 和工具配置，没有公开外部模型供应商配置。
响应中的 `model.name` 也不构成写入外部模型的证据。

## 验证与剩余边界

自动夹具覆盖活动个人账号定位、企业账号拒绝、版本限制、单模型选择、元数据缺失保护、
配置保留、重连复用 Key、取消恢复、账号切换后测试失效以及显式测试边界。
桌面 UI 覆盖安装与接入分离、单模型测试选择器、重启提示。
本次 Core 全量回归 `228 passed / 1 ignored`、桌面 UI `25 passed`；
CLI 测试与桌面原生编译检查通过。

另实际运行官方 `1.0.89.447` 的 `dumate-main-server`，使用独立临时 XDG 目录和
本地 OpenAI 模拟端点，验证偏好加载、`legacy_custom` 模型/密钥覆盖、普通和流式转发。
macOS 沙箱禁止外网及读取 `/Users`，没有使用真实百度或 Token Market 凭据。
见 [原生后端证据](acceptance/2026-11-03-dumate-macos-runtime.json)。
此证据不等于完整 Agent 工具执行循环或真实 Token Market 模型验收。

真实模型请求、完整 Agent 工具调用和 Windows 原生安装/运行需分别验证。
