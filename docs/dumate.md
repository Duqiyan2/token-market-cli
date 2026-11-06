# 百度搭子 DuMate 个人版接入

核实日期：2026-11-06。当前采用一套统一的冷启动接管流程：保存配置与账号级引擎扩展，重启后将文本和工具任务的模型调用统一发往平台及所选模型。个人版一次选择一个模型。

统一扩展转发时原样保留搭子的 `X-Dumate-Session-Id`，由网关兼容读取，用于多轮和恢复会话关联。范围、生效步骤和验证见[Agent 会话 ID](session-ids.md)。

## 使用与提示

1. 在工具管理选择接入或重新接入，选择模型后保存；详情中可以切换模型或取消接入。
2. 无论搭子是否运行，都可以加载模型列表并保存接入、重新接入或切换模型配置；选择和保存不读取当前进程的运行模式、任务状态或账号环境，不要求先退出搭子。接入前仍确认已安装引擎和磁盘上的个人版配置。保存后只读检测搭子主进程和后台：仍在运行时提示“已保存，重启搭子后生效”；未运行时提示“已保存，下次启动搭子时生效”，不显示重启按钮。检测失败不影响已保存配置，提示用户按实际运行状态退出后重新打开。保存同时更新原生偏好、统一接管扩展和账号私有文本策略；不更新正在运行的实例，不自动重启或调用真实模型测试。
3. 搭子运行时，成功弹窗提供“重启搭子”和“稍后手动重启搭子”；未运行时仅提供完成或知道了。保存与重启是独立操作；未点击重启时不检查任务状态、不退出应用。重启成功后自动关闭弹窗，不追加按钮下方的成功文案；失败原因显示在按钮上方。稍后按钮采用原生弹窗关闭，Esc 也可关闭。
4. 用户点击“重启搭子”后，统一弹出确认：“所有正在运行的任务都将被中断，定时任务也将无法调度。”，提供“强制重启搭子”“取消”。不判断任务是否运行、是否空闲或是否可读取；取消不改变已保存配置，也不结束进程。
5. 用户明确选择强制重启后，核对当前系统用户、安装路径、PID 和启动时间，仅结束该搭子安装的主程序及附属进程。确认完全退出后重新打开；身份不明、实例发生变化或结束失败时停止并显示原因。

切换模型复用原 Key、连接归属和计费空间。旧的纯配置连接在下一次切换模型或重新接入时自动迁移，不提供两套模式。取消接入恢复接入前管理字段、移除接管文件，保留平台 Key；仅在搭子仍运行时需重启以卸载当前实例内已加载的扩展。
配置冲突按原恢复规则提示，不覆盖用户后续修改。CLI 的 `tokenmarket connect dumate`
仍保存配置，重启由用户完成。企业版保留独立适配，未纳入个人版免退出保存流程。

## 独立重启的范围

本机已安装兼容引擎时，可用隔离子进程回归生产目录条件下的选择、接入、切换、重新接入和取消流程：

```bash
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --test dumate live_runtime_identity_does_not_block_saved_model_configuration -- --ignored --exact --nocapture
```

该测试只读取已安装引擎的接入能力，使用临时账号目录、合成运行中 PID 和本机模拟平台，不启动或退出真实搭子，不调用付费模型。

重启不再读取任务状态、进程环境或本机鉴权凭据，不调用任务查询和引擎关闭接口。`OPENCODE_NEW_RUNTIME=0/1` 不再作为重启条件；保存配置仍与重启独立。

macOS 使用系统进程接口按当前用户、安装包内的可执行路径、PID 与微秒级启动时间登记进程。确认后先发送 TERM，短暂等待后对原实例中仍存活的进程发送 KILL；每次发信号前重新核对身份，不按进程名批量结束，也不结束其他安装或系统程序。
Windows 使用当前用户、可执行路径、PID 与启动时间登记进程，并核对 Restart Manager 返回的受影响集合；明确确认后使用 `RmForceShutdown` 结束所登记的实例，不登记文件或系统服务。进程身份与完全退出检查保留，不读取进程内存。Windows 已通过交叉编译，尚未完成真机验收。

两端都等待所属进程完全退出后才重新打开。配置保存结果不会因用户取消、实例变化或重启失败而回滚，不重新启动 Windows 或 Token Market 客户端。

## 单一方案与旧连接迁移

保留原来的模型列表、平台密钥复用、账号和计费归属、配置恢复事务、独立重启交互。
连接、重新接入和切换模型使用同一保存路径，不保留可选的旧接入模式。
原生偏好与以下两个账号级文件一并提交，失败时回滚；不修改安装包、用户历史任务库或生成的 `opencode.json`：

- `config/opencode/plugins/tokenmarket-dumate-unified.js`：不含密钥的引擎扩展。
- `config/opencode/tokenmarket-unified.json`：私有文本模型配置，保存平台地址、密钥、模型与限制及指纹。通过既有安全写入生成，macOS 权限为 `0600`；连接记录及恢复信息继续由既有加密存储保护。

搭子原生 `customLLMProvider` 会连 `model-mm` 也转到自定义平台，因此接入时将其设为 `null`（接入前值由恢复记录保留）。图片和视频保留原生官方模型路径，平台密钥不用于媒体请求。
扩展在引擎启动时读取一次账号私有文本策略。当前进程保留这一快照，后续保存只有重启才加载。
在实际模型请求发出前统一替换平台地址、认证、模型和输出上限；标题、摘要和本地子任务也经过此入口。
`chat.message` 同步设置固定模型选择，避免旧任务追问被路由到云端助手；远端路由预检在引擎内按固定选择返回，
无需调用分类模型。仍需本地分类的标准模型请求也通过统一出口。
不转发百度 Cookie 或内部鉴权到平台。转发直接使用 HTTPS，不依赖系统代理；拒绝重定向。
扩展没有监听端口、常驻代理进程、热更新队列或控制服务，不依赖管理工具保持打开。

图片/视频生成接口、图片/视频输入和原生 `qianfan-multimodal` 模型继续走官方路径，保留原 URL、模型及认证；不作为文本失败的回退出口。
文本模型的未知协议、云端助手执行入口或策略不一致均明确失败，不转回官方文本模型。
当前适配要求模型支持 OpenAI Chat Completions 文本和工具调用。该范围不等于接管官方图片/视频模型、登录、技能云同步、
搜索等非模型业务，也不表示完整云端助手专有工具可以原样迁移到本地执行。

取消接入时移除本工具扩展；用户修改过且仍带本工具标记的扩展会移至账号内
`state/tokenmarket-dumate/retired-unified-<sha256>.js.disabled` 备份。
第三方替换文件保留；原生偏好和策略字段存在冲突时沿用原恢复确认流程。

旧热更新实验桥接仍停用。桌面管理工具启动，以及个人版保存/取消前，识别本工具标记的
`config/opencode/plugins/tokenmarket-dumate.js`，先禁用旧策略再移至
`state/tokenmarket-dumate/retired-bridge-<sha256>.js.disabled`。
保留原生偏好和备份，不处理第三方扩展。已加载旧代码需完全退出后才能卸载。
新的冷启动接管使用独立文件和标记，不恢复旧热更新机制。

## 兼容性与验证范围

接入、重新接入、加载模型和切换模型均按当前安装的引擎能力检查，不要求版本号、build 或引擎 SHA256 与安装器一致。Windows 的 `1.0.89.0`、用户升级或降级不会仅因版本不同被拒绝，也不会触发自动升降级。

保存前只读扫描引擎内嵌代码，确认账号级扩展加载、配置/消息/参数钩子、个人文本与原生媒体来源、固定模型选择、任务路由和请求拦截入口。缺少入口时列出未检测到的能力，在申请平台 Key 和修改配置前停止。该检查是结构预检，不能证明任意版本的完整运行语义。

扩展启动时不再限定 `DUMATE_APP_VER` 或 `OPENCODE_NEW_RUNTIME` 数值，而是验证实际个人版提供方、`glm-5` → `model-text` 映射、原生模型入口、文本策略指纹和已停用的原生全局覆盖。实际接口不兼容时明确报错，模型请求不回退到官方文本模型。仍保留个人版/企业版边界。

macOS 和 Windows 的官方 `1.0.89.447` 引擎已进行只读能力扫描；升级、降级以及四段版本号由隔离回归覆盖。完整原生路由验收仍仅覆盖下述已测试安装，不代表所有新旧版本均已完成真机验收。重装保留账号数据时扩展可保留，清除账号数据则需重新接入。Windows 重启只核对应用身份和所属进程，不再绑定固定版本或 build。

其他第三方引擎扩展暂拒绝混用，防止后加载的代码改变模型出口。

隔离原生验收：

```sh
python3 tests/helpers/dumate-unified-native-probe.py
python3 tests/helpers/dumate-unified-native-probe.py --runtime 1
```

该探针运行已安装的官方 macOS 后台与引擎，使用临时账号、假密钥、本机模型服务，
沙箱禁止外网和读取真实用户目录。验证旧任务、新任务、实际 explore 子任务、标题、显式摘要，
以及切换后重启、重新接入后重启、取消后重启。额外注入一份**合成的已完成云端文本历史**，
验证旧任务续问保留文本并改走所选模型；未对真实完整云端任务迁移作承诺。
图片与视频分别验证为原生媒体模型及原生认证，媒体请求未使用平台密钥；生成接口的原样放行另由单元测试覆盖。
文本上游失败不回退、错误策略零模型请求、路由预检含 scheduler 重映射、输出上限和保存后快照隔离均通过。
结果见 [统一接管验收](acceptance/2026-11-06-dumate-unified-cold.json)。

2026-11-06 补齐新引擎模式 `1` 的接管支持，修复扩展初始化时报“当前搭子版本或运行模式不支持统一接管”的 500。模式 `0`、`1` 各完成同样的 18 阶段隔离原生回归，覆盖旧/新任务、子任务、标题与摘要、保存快照、切换/重新接入/取消后的冷启动、图片/视频原生路由与错误配置零请求。详见 [双模式验收](acceptance/2026-11-06-dumate-runtime1.json)。安装包含本次修复的客户端后，需重新接入并重启搭子，让磁盘扩展与进程快照一并更新；仅升级客户端不会覆盖已有接入扩展。该修复不改变搭子当前的引擎模式。

当前强制重启回归使用 `cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --lib dumate::restart`。macOS 夹具只结束临时编译、位于临时应用目录且忽略 TERM 的测试进程，验证 KILL 后退出、PID 启动时间不符时拒绝结束、无关进程继续运行。界面覆盖固定警告、取消、Esc、退出登录及接入/切换/取消接入后的重启。

原任务状态探针、[交互验收](acceptance/2026-11-06-dumate-restart-confirmation.json)和[Windows 重启验收](acceptance/2026-11-06-dumate-windows-restart.json)保留为历史记录，不再描述当前重启前置条件；[弹窗关闭验收](acceptance/2026-11-06-dumate-dialog-close.json)保留对应界面历史。
旧纯配置及热更新记录仅为调查历史，以统一接管验收为当前模型路由范围。

所有验证未调用真实付费模型、未改真实用户任务或配置、未重启真实搭子。
Windows 原生运行、真实模型兼容性和真实云端历史任务续问仍需设备验收。

Windows 开发机可先运行无真实搭子操作的原生检查：

```powershell
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --lib dumate::restart
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --lib dumate::restart::windows::tests::restart_manager_closes_only_own_window_fixture -- --ignored --exact
```

第二条只建立并结束临时自建窗口，用于验证 Restart Manager 的受限进程集合及退出回调，不关闭真实搭子。
Windows 设备验收应核对固定警告、取消/确认、取消接入后的重启和搭子已退出时重新打开；
确认主进程、引擎、后台均退出后才拉起一个新实例。进程身份不明或无法结束所属进程时应提示手动重启，不再查询任务状态。

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

配置保存不调用连接检查或真实模型，不调用运行时设置接口，不改当前后台内存、生成的引擎配置或历史任务记录。
当前实现将原生全局自定义覆盖停用，将文本配置保存到账号私有策略并安装上述冷启动扩展，下一次启动时统一文本模型出口。下文原生路由描述是客户端证据，不能单独证明统一接管范围。

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

版本兼容性回归：

```sh
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --test dumate --test dumate_compatibility
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --lib dumate::unified
node --test tests/dumate-unified.test.cjs
```

引擎可用以下只读检查验证；不执行目标引擎、不访问账号或模型：

```sh
TOKEN_MARKET_DUMATE_ENGINE="<引擎可执行文件路径>" cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --lib dumate::unified::tests::installed_engine_capabilities -- --ignored --exact
```
