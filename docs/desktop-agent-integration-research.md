# 桌面 Agent 全模型路由调研

调研日期：2026-11-06。范围：千问办公 QwenWorkCN、百度搭子 DuMate；公开官方产品文档与网络平台文档。本文仅记录外部证据与工程判断，不把某一桌面版本的私有实现当作官方承诺，也未执行真实任务或核验积分账单。

后续用户决策：暂不推进统一接入方案，按 App 分别处理，优先少改底层和稳定运行。接入、取消先保存配置，不要求用户先退出；保存与重启分开，提示重启后生效，由用户点击按钮请求千问正常退出，后续确认和任务收尾交给千问 App。生图和视频暂不处理。必须新增深改才能接管的入口，应先列出具体方案和影响，由用户决定。下文的统一架构建议属于调研备选，不代表已批准的实施计划。

## 结论

**建议共用原生模型中继核心，并保留每个产品的接入适配器；不能把通用网络代理作为“稳定接管全部收费模型调用”的承诺。** 这是工程判断：网络可达性、TLS 解密、产品协议适配、任务中的调用覆盖、最终计费是不同问题。底层流量经过本机，不等于能替换其模型；本机甚至看不到供应商云端内部发起的模型子调用。

最稳定的接入顺序应为：已开放的官方 BYOK → 经版本匹配验证的本地 provider/worker 接入 → 明确标注支持范围的实验方案。企业服务端 BYOK 与本地 worker 中继是两条不同部署路径，不能都配置成 `127.0.0.1`。

## 产品证据与未知项

| 产品/路径 | 官方文档确认的能力 | 仍不能据此承诺的事情 |
| --- | --- | --- |
| 千问办公企业 BYOK | 已开通权益的企业可配置完整 Chat Completions Endpoint、API Key、服务端模型 Key；主模型与视觉、语音、多媒体专项模型分别配置。官方要求外部模型费用在服务提供方核对。 | 个人版 QwenWorkCN 具备相同入口；只改主模型即可覆盖分类、标题、压缩、规划等所有辅助调用；零官方积分。 |
| 千问办公企业 BYOK 部署 | 开发者文档明确调用方向是 **QwenWork 服务端 → 客户接口**。模型地址须从 QwenWork 服务所在网络可达。 | 该路径可以访问用户电脑的 loopback；它与当前个人桌面直接调用路径相同。 |
| 百度搭子开放平台 | 官方 OpenAPI 提供会话、异步 prompt、SSE 等；每轮 `model.providerID/modelID` 可选，不传时使用服务端默认模型。 | 该 API 的模型选择等于桌面任意 Endpoint/BYOK；桌面自定义 provider 覆盖所有辅助调用；云端任务内部模型调用可以在本机被逐次替换。 |

千问办公产品证据：[阿里云官方模型目录](https://help.aliyun.com/zh/qwenwork/enterprise-ultimate-models-model-catalog)、[官方接入总览与接口清单](https://docs.qwenwork.cn/developer/integrations/overview)。企业文档的适用范围应保留，不能外推到个人版或所有安装版本。

千问办公的[官方模型 BYOK 与搜索接口](https://docs.qwenwork.cn/developer/integrations/model-byok/byok-and-search)进一步区分 Chat/VL/语音润色、图片、视频、音乐、ASR、搜索协议；Chat Agent 还要求工具调用与 SSE 语义。**工程推论**：共用中继可以复用鉴权、路由与观测，但不应把所有能力机械转换为同一个文本 Chat 请求。文档没有列出所有隐藏辅助调用，也没有给出全量扣费豁免承诺。

百度搭子证据：[官方标准化 OpenAPI 接入指南](https://baidumate.com.cn/doc/ymu8cfo0x.html)。本次直接抓取该站页面超时，以上仅使用搜索索引返回的同一官方页面正文；未获得完整桌面 BYOK 契约。未发现公开承诺不等于产品没有相应私有能力，需以目标版本本地证据和验收补充。官方的[连接应用管理](https://baidumate.com.cn/doc/qmtzjdkyk.html)描述的是外部应用/MCP 服务配置，不能拿它证明主模型替换能力。

## 为什么网络代理无法消除产品适配

1. **Electron 不是单一网络栈。** `session.setProxy` 为对应 Session 设置代理，连接池可能还需关闭旧连接；`ses.fetch` 使用 Chromium 栈，Node `fetch` 使用 Node HTTP 栈。[Electron Session 官方文档](https://www.electronjs.org/docs/latest/api/session#sessetproxyconfig) [Electron net 官方文档](https://www.electronjs.org/docs/latest/api/net)。**工程推论**：配置一个 Session 不能证明已覆盖其他 partition、Node 子进程、外部 worker 或原生组件；每个入口仍需查证。

2. **CONNECT 只建立隧道。** RFC 规定其成功后双向盲转发，HTTPS 可在隧道内保持端到端 TLS。[RFC 9110 §9.3.6](https://www.rfc-editor.org/rfc/rfc9110.html#section-9.3.6)。**工程推论**：普通 HTTPS 代理看不到 JSON 中的 `model`、消息或工具调用，不能仅凭改网络目的地完成 Token Market 协议替换。TLS 中间人需让对应客户端接受中继证书；Node 官方示例允许另外校验证书/公钥指纹，说明即使证书链受信也可能被 pinning 拒绝。[Node HTTPS 官方 pinning 示例](https://nodejs.org/api/https.html#httpsrequestoptions-callback)。这证明机制存在，不证明这两款产品实际启用了 pinning。

3. **操作系统层能力解决连接覆盖，不解决模型语义。** Apple 明确要求 packet tunnel 用于隧道，反对拿它托管代理、选择性接流后再绕行；macOS 透明代理应考虑对应 App/Transparent Proxy API。[Apple TN3120](https://developer.apple.com/documentation/technotes/tn3120-expected-use-cases-for-network-extension-packet-tunnel-providers)。Network Extension 还涉及相应 entitlement、签名与部署方式。[Apple entitlement 文档](https://developer.apple.com/documentation/BundleResources/Entitlements/com.apple.developer.networking.networkextension) [Apple TN3134](https://developer.apple.com/documentation/technotes/tn3134-network-extension-provider-deployment)。Windows WFP 可通过 ALE callout driver 把连接重定向至代理，需处理重定向记录与循环。[Microsoft WFP 重定向文档](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/using-bind-or-connect-redirection)。**工程推论**：这引入两套平台工程，仍需 TLS 与产品协议适配，并不比显式模型接入天然稳定。

4. **云端子调用不在客户端连接中。** 客户端向供应商发起一次任务请求后，供应商服务端可以自行调用分类、摘要或主模型；本机代理只能处理经过本机的请求。**工程推论**：若这类调用没有官方 BYOK/配置出口，只能替换整个任务服务协议并重做其行为，不能在客户端逐模型重写。Qwen 企业 BYOK 的官方服务端调用方向也是需要区分部署拓扑的直接例子。

## 建议的共同边界与验收

以下为工程建议，尚非已实现或已验收能力：

- 共用核心负责 Token Market 凭据、模型映射、流式转发、取消/超时、错误处理与调用证据；本地路径只监听 loopback，并使用局部授权。服务端 BYOK 则使用供应商服务网络可达的服务地址。
- 产品适配器负责发现实际模型入口、版本识别、配置与恢复、worker 生命周期及辅助调用策略。优先利用官方契约；依赖私有 bundle/worker 的方式必须限定已验证版本，变化时停止接管并显示原因。
- 区分“对话能回答”“当前任务所有已知模型调用已接管”“供应商无模型积分扣费”三个验收结论。分别保留中继请求证据、Token Market 用量和供应商用量明细，按同一任务时间窗口对账。
- 覆盖主对话、工具循环、分类/路由、标题、上下文压缩、子任务、重试、续接、后台/定时任务及实际使用的视觉/语音能力。若某一路由无法确认，报告未覆盖；不能以一个成功 Chat 或一次余额未变证明全覆盖。
- 若目标是防止官方收费模型回落，受控的模型入口缺失或中继故障时应明确失败；登录、更新、文件与工具服务独立按产品需要放行。供应商云端内部调用若没有可控出口，必须保留“无法保证全部转接”的结论。

本次官方资料足以否定“一个通用系统代理即可稳定保证两款产品全量收费调用替换”的推断，但不足以判断两款具体桌面版本全部辅助调用的实际位置、可控性或计费结果；这些应由本地源码证据和真实任务账单验收补全。

## 当前仓库实现复核

以下复核针对 2026-11-06 工作区，包括尚未提交的改动；不是已发布安装包的状态。本轮只新增研究记录，没有继续改动接入代码或真实客户端。

| 项目 | 当前实现 | 稳定性判断 | 覆盖与验收边界 |
| --- | --- | --- | --- |
| DuMate 个人版 | 向账号级 `dumate-app.json` 写 `customLLMProvider`，使用厂商原生启动读取、Go 路由及显式重启；当前未注册旧扩展或热更新桥 | 接入行为少，现阶段比接管厂商内部执行流程更易维护；但这是从特定安装版本核实的私有持久配置，并非公开长期兼容承诺 | 不强制改变 `ai_assistant` 云端助手路由；本地自定义模型成功不能推出所有正文或辅助模型已接管 |
| QwenWorkCN 个人桌面 | 复制并适配外置 SDK worker，用占位凭据连接原生中继；新增桌面 MaaS 签发入口接管；原始 SDK 网关回退指向保留无效域名 | 真实平台 Key 不进入厂商 SDK，应用包保持只读；但增加了私有源码匹配、两个调用入口、模型别名和启动时序依赖 | 证明了已识别接口的离线行为，未证明完整 App 所有模式、媒体模型、缓存/升级行为或官方积分账单全部通过 |

代码与本地文档依据：

- [DuMate 配置适配](../rust/tokenmarket-core/src/connections/dumate.rs)：`apply` 仅写一个原生 `customLLMProvider`；`apiKey` 是真实平台 Key，当前直接配置平台 `/v1`。因此它尚未共享千问的“厂商仅持有本地访问令牌”模型。
- [DuMate 接入记录](dumate.md)：明确保留云端助手绕行及输出上限限制。旧热更新、桥接、冷/热启动实验记录已标记 superseded；未被生产适配注册的临时探针不能作为当前方案已全覆盖的依据。
- [千问 worker 合同](../rust/tokenmarket-core/src/qwenwork/contract.rs)及[运行时生成](../rust/tokenmarket-core/src/qwenwork_runtime.rs)：识别源码的字面结构并替换调度位置；拒绝不匹配比猜测继续运行可靠，但厂商代码结构变化仍可能要求适配。
- [千问接入配置](../rust/tokenmarket-core/src/connections/qwenwork.rs)：复核时辅助模型取目录首项。后续稳定性修复已改为保留仍可用的辅助模型，缺失时优先客户端默认模型，再按模型 ID 稳定选择；不再由目录排序决定。仍写 `ENGINE_MAAS_MINT_URL`、路由文件位置与 SDK 备用网关。
- [启动逻辑](../rust/tokenmarket-core/src/qwenwork/host-bootstrap.mjs)：桌面代理由 worker 首次触发启动，多个 worker 复用，原生进程监测宿主退出。后续稳定性修复已隔离辅助代理启动故障，不再因此终止整个 worker，SDK 继续使用独立的平台本地路由。主进程先发辅助请求或原生进程回退等顺序仍需完整 App 验证。
- 后续保存/重启修正：接入配置与取消配置允许运行中持久化，已启动的代理保留其加载的原连接；完整重启后加载新配置。路由文件不再跟随 SDK 版本目录变化，新增可空/有默认值字段和列顺序变化不再直接拒绝，未知必填或核心字段变化仍拒绝写入。重启按钮仅使用系统正常退出请求，不做内部任务查询或强杀。
- [桌面中继](../rust/tokenmarket-core/src/connections/qwenwork_relay/host.rs)：只处理经过该回环入口的 Chat Completions、Responses、Messages；未知接口返回错误。它不是全进程网络过滤器。当前 `TEXT_GENERATION` 标签被用于接受文本协议，不能替代各协议在真实模型上的适配验证。
- [桌面离线探针](../tests/helpers/qwenwork-host-relay-probe.mjs)：提取真实 `send` 方法，但人为提供 route、credential、strategy，并循环五个 purpose 字符串；这验证共同发送路径，不是分别触发五种原生桌面功能、其前置路由和官方计费链路。

对本机 `/Applications/QwenWorkCN.app/Contents/Resources/app.asar` 的 `out/main/main.js` 另做了只读检查：

1. `resolveMediaGenerationGatewayPolicy` 在 `skills.showEnterpriseMediaGenerationSkill` 策略命中时可选择 `openapi-v1`。
2. `OpenApiMediaGenerationGateway.submit` 使用 `fetchOpenApi(MEDIA_GENERATION_PATH)`，区别于桌面 MaaS credential provider。
3. `getEngineMaasBaseUrl` 的优先级是进程环境 `ENGINE_MAAS_BASE_URL`、签发响应的 `serverProvidedBaseUrl`、默认地址。

上述是安装包静态分支证据：没有触发媒体生成，也没有据此断言当前账号启用了该分支或已经扣费。它足以说明仅配置 MaaS 签发地址与 SDK 网关，不能推出所有其他出口被拦截；“不支持的接口会失败”只适用于实际进入中继的调用。

## 建议落地顺序

1. **先定义接入能力与验收状态。** 分别记录主对话、标题/建议、压缩/摘要、子 Agent、视觉/语音/生图、云端任务的路由位置、可控入口和证据。状态至少区分已验证、有限覆盖、未知/不支持。官方模型推理费用与厂商可能存在的其他服务费用分别核验。
2. **保留两款产品各自最少侵入的入口。** DuMate 优先保留持久配置与重启；要全量本地接管时，单独核实并控制云端助手模式。千问继续使用目前已识别的外置 worker 和辅助入口，但定位为版本相关适配，补齐其他出口调查及真实应用验收后再扩大承诺。企业官方 BYOK 使用服务端可达的 Token Market 兼容接口，不套用本地 loopback 方案。
3. **统一真正共用的推理底座。** 收敛本地鉴权、每工具/账号/空间的凭据隔离、模型能力、协议、SSE、取消/重试、计量、配置事务与恢复；产品适配只负责发现、安装/卸载接管配置、入口和生命周期。简单且完整的官方直连模式不必为了形式统一强加常驻代理；若要求真实平台 Key 不进入第三方 App，再将其配置到共同的本地中继。
4. **使用明确的模型任务策略。** 主模型、辅助文本、视觉分别映射到明确配置，避免用模型目录首项充当默认。未知协议或不可控云端模式标为不支持；受控入口失效就停止该调用，不能自动使用官方付费模型。可独立识别的推理网络出口可加阻断作为补充，但共享业务域名和厂商云端子调用不能靠本机域名过滤保证。
5. **建立完整 App 的兼容验收。** 使用隔离数据目录/本机上游验证新旧会话、恢复、工具循环、压缩、子任务、辅助入口、代理未启动/崩溃、账号/空间切换、更新后入口变化。随后由授权的真实任务核对平台模型账单、官方模型明细和延迟归集后的积分。未经该层验证，不标记“全部收费模型调用已接管”。

该方向统一的是接入架构、公共中继和验收规则，不能免除每新增一款产品的入口适配。若硬性要求任意 App、任意版本、功能全部可用且厂商模型积分绝不扣减，现有公开契约和本地证据都不足以支持这样的保证。
