# 桌面用量统计：现有 Web / Server 能力核查

核查日期：2026-11-05。以下保留调研及方案收敛过程。后续已完成本地代码实现，当前行为、接口与验证结果见 [桌面用量统计](desktop-usage.md)；未部署。

已确认的产品范围：**首版只统计通过 Token Market 产生的用量**，不包含第三方直连或官方订阅用量；服务端记录作为主数据源，上方四项汇总与按日趋势，下方直接展示详细分页日志，支持时间、工具、模型和状态筛选。最新用户澄清将范围定为：**当前账号＋当前空间＋当前设备下曾接入工具的全部 API Key，包括同一工具历次创建的 Key；其他设备排除。** 不要求工具现在仍处于接入状态。用户同时提出尽量将汇总和趋势计算放在客户端，以减少服务端压力；时间桶＋客户端合并是候选实现。Token Market Usage 与 Device Tool Usage 已记录在根目录 GLOSSARY。

结论：Server 已有请求明细和聚合统计服务，足以作为 Token Market 流量统计的主要数据来源；桌面现有 `session_token` **不能直接调用当前 Web 日志和统计接口**。可考虑增加 `/api/client/*` 下只读桌面接口并复用 domain 服务，但权限、统计范围与指标口径仍需产品确认。

本次只核查本地源码；两仓均无 `.codegraph/`。`http://localhost:9000/api/meta/pages` 连接失败，未验证部署、实时数据、真实账号权限、日志保留期与聚合延迟。没有读取或输出凭证。

## 源码根目录

- Server：`/Users/bingbing.chen/mying/llm-platform`。
- Web：`/Users/bingbing.chen/mying/llm-platform-web`。
- 下文链接均为绝对路径，链接文字是各仓库内相对路径，行号对应核查时工作区。

## 已有接口与数据

| 接口（Controller mapping） | 能力 | 边界 |
| --- | --- | --- |
| `GET /api-logs` | 当前授权空间请求明细、总数、同条件汇总；页码或 cursor | Session only；不是“当前设备 / 当前工具”统计 |
| `GET /api-logs/summary` | 请求数、实际额度、输入/输出/总 Token | 无 cache、平均 TTFT、TPS 汇总 |
| `GET /metrics/overview` | 请求/成功/失败、token/cache、费用、成功率、平均耗时、环比、今日、趋势和排名 | 单选空间；24h/7d/30d；无自定义起止时间、key/device/tool 过滤 |
| `GET /metrics/usage` | summary、小时序列、模型分布、API key 排名；可按单个 apiKeyId 过滤 | 1h/3h/12h/1d/3d/7d/30d；没有 tool/device 过滤 |

依据：

- [web/apilog/controller/ApiLogController.java:33](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/apilog/controller/ApiLogController.java:33)，33–55 行定义明细和汇总。
- [web/metrics/controller/MetricsController.java:42](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/metrics/controller/MetricsController.java:42)，42–85 行定义统计参数。
- [api/metrics/model/MetricsOverview.java:24](/Users/bingbing.chen/mying/llm-platform/llm-platform-api/src/main/java/com/prosperllm/llmplatform/api/metrics/model/MetricsOverview.java:24)，24–109 行定义 summary/comparison/today/trend/rankings。
- [api/metrics/model/MetricsUsage.java:75](/Users/bingbing.chen/mying/llm-platform/llm-platform-api/src/main/java/com/prosperllm/llmplatform/api/metrics/model/MetricsUsage.java:75)，75–94、172–188、258–272 行定义 usage 响应。
- [common/constants/metrics/MetricsConstants.java:11](/Users/bingbing.chen/mying/llm-platform/llm-platform-common/src/main/java/com/prosperllm/llmplatform/common/constants/metrics/MetricsConstants.java:11)，11–25 行定义业务时区、窗口和维度。

日志明细已有 `request_id/workspace_id/api_key_id/api_key_name/api_key_masked/user_id/group/channel/model/type/rsp_code/error_reason`，`duration_ms/rsp_ttft_ms/stream`，`input_tokens/output_tokens/total_tokens/cached_tokens/cache_creation_input_tokens/client_endpoint`，`consumed_amount_micro`、计费/折扣快照、IP、开始结束时间。尚无 `tool/device/client_id/session/project` 字段，也无每请求 TPS 字段。

依据：[web/apilog/model/ApiLogQueryParam.java:173](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/apilog/model/ApiLogQueryParam.java:173)，173–295 行。

## 鉴权与数据范围：桌面不能直接复用 Web HTTP 接口

1. `/api-logs` 与 `/api-logs/summary` 标记 `SESSION_ONLY`。AuthAspect 在没有有效浏览器 HttpSession 时直接拒绝，不继续校验 Bearer。
2. 桌面 Bearer token 有独立 namespace / 撤销生命周期；即使是默认 `SESSION_FIRST` 的 metrics，AuthAspect 也只接受它访问 `/api/desktop/*` 或 `/api/client/*`。
3. 因而不能把现有桌面 session token 填进 Authorization 后直接调这些接口，也不应为了统计而放宽所有 Web 接口的 audience。

精确依据：[web/base/aspect/AuthAspect.java:428](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/base/aspect/AuthAspect.java:428)，428–441 行 Session-only 分支，463–480 行桌面 audience 限制；此仓没有 `DesktopSessionAuthFilter`，实际所有者是 AuthAspect。

日志查询通过 `WorkspaceScopeGuard` 从当前鉴权上下文的 `logWorkspaces` 解析目标；`self` 是“目标空间本身”，并不是“当前用户自身”。`visible_tree` 仅以一级空间为根，纳入该空间及授权直属子空间，不信任客户端给出的 workspace ID 列表。概览则校验 `overviewWorkspaces`，代码明确说明概览准入 `console:overview:view` 与日志 `console:logs:view` 应分开。新增 desktop facade 也必须保留这些 live permission 检查，不能因允许管理 key 就推断允许读取企业全部日志。

依据：[web/auth/service/WorkspaceScopeGuard.java:85](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/auth/service/WorkspaceScopeGuard.java:85)，85–136 行；[web/metrics/controller/MetricsController.java:72](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/metrics/controller/MetricsController.java:72)，72–95 行。

已有可复用 domain 契约：[api/apilog/service/IApiLogQueryService.java:9](/Users/bingbing.chen/mying/llm-platform/llm-platform-api/src/main/java/com/prosperllm/llmplatform/api/apilog/service/IApiLogQueryService.java:9) 的 query/summarize，以及 [api/metrics/service/IMetricsService.java:18](/Users/bingbing.chen/mying/llm-platform/llm-platform-api/src/main/java/com/prosperllm/llmplatform/api/metrics/service/IMetricsService.java:18) 的 overview/console usage。它们不会替新 HTTP facade 自动完成用户/空间授权。

## 时间、分页和聚合口径

- 日志起止时间均为毫秒且包含端点，按 `request_started_at`（为 0/null 时回退 `created_at`）过滤。page 从 1 开始、默认 20、最大 100；支持 `next_cursor`，`include_summary=false` 可跳过总数和汇总。筛选模型/key 名称模糊匹配、结果类型、group、精确 request ID、channel ID/名称。当前日志契约没有精确 `api_key_id` 或 ID 集合过滤，若要按绑定 key 严格限缩当前设备/工具需要扩展，不能以名称模糊查询代替。
- 日志 SQL 的总 Token 汇总是 `SUM(total_tokens)`，不是重新计算 `SUM(input+output)`。HTTP Summary 注释仍写“输入+输出”，与真实 SQL 不一致，Anthropic cache 场景不能照抄该注释。
- overview 从 `metrics_usage_stats_minute` 聚合，半开区间 `[start,end)`，端点向下一个五分钟边界对齐；趋势为五分钟桶、按总 Token 排序选 top 5 加其他项；排名 top 10，模型目录查询上限 10,000。维度是 model/vendor/apiKey，**没有按工具、设备、group 聚合**。
- overview `successRatePct = successCount / requestCount × 100`；`avgLatencyMs = round(sum(rsp_time_ms)/requestCount)`。不应当作成功请求样本平均值或 P50/P95。今日边界为 Asia/Shanghai。
- `/metrics/usage` 是另一条小时 rollup：end 为当前小时起点，start 为 `end-duration+1h`；不要把响应 endTime 误当精确到现在的最后事件时间。其平均耗时同样除以总请求数，模型 share 按请求数计算。

依据：

- [web/apilog/model/ApiLogQueryParam.java:26](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/apilog/model/ApiLogQueryParam.java:26)，26–164 行；[common/constants/apilog/ApiLogConstants.java:8](/Users/bingbing.chen/mying/llm-platform/llm-platform-common/src/main/java/com/prosperllm/llmplatform/common/constants/apilog/ApiLogConstants.java:8)。
- [mybatis/mapper/apilog/ApiLogMapper.xml:110](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/resources/mybatis/mapper/apilog/ApiLogMapper.xml:110)，110–159、225–285 行为过滤、游标、排序、汇总 SQL。
- [mybatis/mapper/metrics-stats/MetricsStatsMapper.xml:5](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/resources/mybatis/mapper/metrics-stats/MetricsStatsMapper.xml:5)，5–8、40–124 行为窗口与聚合。
- [metrics/service/MetricsOverviewService.java:43](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/java/com/prosperllm/llmplatform/metrics/service/MetricsOverviewService.java:43)，43–69、95–110、190–198 行为窗口、公式、日界；[metrics/service/MetricsOverviewDimensionService.java:31](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/java/com/prosperllm/llmplatform/metrics/service/MetricsOverviewDimensionService.java:31)，31–33、94–165 行为维度/桶。
- [metrics/service/MetricsService.java:53](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/java/com/prosperllm/llmplatform/metrics/service/MetricsService.java:53)，53–88 行为小时窗口及公式。

## Cache、费用、TTFT 与 TPS 的限制

- 直接明细已有规范化的 read/write cache Token；total_tokens 注释明确 Anthropic 入口包含 cache。不要无条件把 cache 再加到 total；不能假设各协议 input_tokens 的 cache 包含方式相同。
- 日志 summary 没有 cache 汇总；overview 只有 `usageCachedTokens`，没有独立 cache write 汇总。需要新增 cache 命中率时，应先确认分母及不同协议口径。
- 当前 Web RequestLogsPanel 反而从 billing_calculation_lines 提取 cache read，并对**当前页日期组**用 `sum(cacheRead)/sum(inputTokens)`；它不是全量时间段服务器命中率，不能照搬为桌面完整统计。日期组费用、成功率、平均耗时也来自当前页。
- 费用是 Server 记录的 `billing_actual_amount` / `consumed_amount_micro`；当前 Web 除以 1,000,000 展示。它是平台实际消耗，不能用静态模型价格反推替代；货币、折扣和取消请求计费需沿用 Server。
- `rsp_ttft_ms` 是流式首个客户端可见 chunk 延迟；非流式/未首包为缺失。不能将缺失填 0 后参与平均。generic usage/overview 没有 TTFT/TPS 聚合。
- Server 另有**渠道模型质量指标** SQL：仅 output_tokens>0 且 generation_ms>0 的样本累加输出和生成时间；generation_ms 是优先 upstream 响应时长减优先 downstream TTFT 后夹到 0。此处存在上下游时间基准混合，不能未经确认就作为桌面每请求精确 TPS。新增桌面 TPS 应约定 TTFT 是否扣除、缺失行为、失败/取消样本及 `sum(tokens)/sum(time)` 还是均值；聚合 TPS 也不代表并发总吞吐。

依据：[web/apilog/model/ApiLogQueryParam.java:243](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/apilog/model/ApiLogQueryParam.java:243)，243–281 行；[app/components/console/RequestLogsPanel.tsx:292](/Users/bingbing.chen/mying/llm-platform-web/app/components/console/RequestLogsPanel.tsx:292)，292–327、702–724 行；费用转换 260–261 行；[mybatis/mapper/apilog/ApiLogMapper.xml:288](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/resources/mybatis/mapper/apilog/ApiLogMapper.xml:288)，288–332 行。

## 工具与设备归因

Server 已将桌面发出的 key 存为 `source=desktop`、`device_id`、`tool`，API log 有 `api_key_id`。`client_key_bindings` 还保存 client/user/workspace/device/tool/current key/predecessor/generation。**已有服务端归因基础，无需先上本地代理才能按 key 识别工具/设备**。

但现成 HTTP 查询/聚合没提供这些维度与关联；需要增加服务端关联与精确 key 过滤，核查轮换/删除 key 的历史记录是否完整保留。key 被用户复制到别的进程/设备时，归因只能表示“使用该已绑定 key”，不能证明真实发起进程。手工 key 或缺失 metadata 应显示未知。

依据：[client/utils/ClientKeyBuilderUtils.java:10](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/java/com/prosperllm/llmplatform/client/utils/ClientKeyBuilderUtils.java:10)，10–15 行；[mybatis/mapper/client/ClientKeyMapper.xml:20](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/resources/mybatis/mapper/client/ClientKeyMapper.xml:20)，20–38 行；[client/model/ClientKeyBinding.java:8](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/java/com/prosperllm/llmplatform/client/model/ClientKeyBinding.java:8)，8–23 行。

## Web 当前实现与候选方案（未确认）

Web 概览已真实调用 `metricsOverview`，提供 24h/7d/30d 与维度选择；当前日志页用单次授权范围查询、cursor 缓存、同条件 summary 复用。它们可以作为 desktop UI 与数据契约参考，但网页组件和会话方案不能直接移植。

依据：[app/components/console/OverviewPanel.tsx:139](/Users/bingbing.chen/mying/llm-platform-web/app/components/console/OverviewPanel.tsx:139)，139–199 行；[app/components/console/RequestLogsPanel.tsx:414](/Users/bingbing.chen/mying/llm-platform-web/app/components/console/RequestLogsPanel.tsx:414)，414–478、609–613 行。

已确认 Server 为 Token Market 流量事实来源。候选实现：desktop 新增受 desktop grant 和工作区权限约束的只读 facade，复用日志与 metrics 服务并补充精确归因；本地可缓存展示数据。跨第三方直连流量不在首版范围内。本地缓存不作为新的账单账本。客户端汇总可以基于完整覆盖目标范围的时间桶数据，不要求每次全量拉取分页日志。

已确定以当前空间下本设备曾接入工具的全部历史 Key 限缩范围；断开工具和更换 Key 不移除历史用量，其他设备的 Key 排除。剩余实现细节包括权限处理、日期范围、cache 指标口径、TTFT/TPS、刷新和缓存；不再将当前接入状态作为查询条件。

## 对照 CC Switch 后的可行性判断

[上游调研](cc-switch-usage-research.md)确认 CC Switch 同时支持本地代理与会话日志导入。截图中的会话日志及近似速度可以来自后一条路径，不必先实现本地代理。以下成本判断是基于两边源码的工程推论，不是已完成实现或工期承诺。

| 方案 | 可行性与主要工作 | 与已确认范围的适配 |
| --- | --- | --- |
| 本地解析工具会话 | 可行；按工具适配格式、增量扫描、去重、模型定价、历史归属。Token Market 当前没有通用会话用量采集服务 | 仍需区分同机直连渠道和平台请求；单靠会话日志无法可靠还原平台扣费，不作为推荐主数据源 |
| 所有工具经过本地代理 | 可行；新增统一流量接管及协议处理，并承担进程生命周期、端口、流式转发和断连维护 | 平台已有请求与费用记录，仅为统计再加一层采集成本较高；也不能恢复未经过该代理的历史或其他设备流量 |
| 服务端查询，桌面原生展示 | 推荐；新增桌面只读接口，复用现有查询/聚合，补工具归因与统一过滤口径 | 按本次确定的账号、空间、设备限制历史 Key 范围，费用沿用平台消耗；桌面可以缓存结果 |

本地代码依据：[Codex/Claude 直接写网关地址](../rust/tokenmarket-core/src/connections/configuration.rs#L260-L340)、[管理请求携带桌面会话与空间](../rust/tokenmarket-core/src/desktop_workspace.rs#L75-L104)、[连接保留 Key 与绑定空间](../rust/tokenmarket-core/src/connections.rs#L72-L103)。已有[千问办公专用 relay](../rust/tokenmarket-core/src/connections/qwenwork_relay.rs#L1-L48)是局部协议/凭据适配，不代表已有覆盖所有工具的用量采集代理。

候选调用链：桌面用量页 → Tauri command → Rust 携带 Client Session 和目标空间 → `/api/client/*` 只读接口 → 实时空间权限校验 → 日志/统计服务。新接口路径与 DTO 尚未定案。无论在哪端计算，不能用当前页日志冒充整个时间段。

### 客户端汇总以降低查询负担（待确认建议）

建议服务端返回权限范围内的轻量时间桶，客户端缓存后合并成四项汇总和按日趋势；请求明细仍按需分页。现有 MetricsService 已读取小时预聚合表，而非每次扫描原始请求日志；可复用此基础，但现成响应的 series/modelDistribution/apiKeyRanking 是各自独立聚合，不能组合还原工具×模型×时间的联合筛选。新增接口需要返回所需联合维度的可加计数，或由服务端先应用筛选，不能把现有排行榜当完整统计底表。

时间桶建议包含请求数、规范化各类 token、平台金额、指标分子分母以及用于去重替换的桶标识；比例由客户端合并分子分母后计算。成本按整数最小单位累计。优先按日提供长期范围，短范围按小时；具体粒度及维度基数需结合数据量评估。服务端鉴权及数据范围裁剪必须先于下发。

历史桶可缓存复用，近期桶需重新校准以容纳延迟入库、请求最终结算或修正；不能仅追加新请求后假定历史永不改变。缓存按环境、账号、空间、授权范围和查询维度隔离，并明确数据截至时间。减少负担的收益来自复用预聚合、减少读取行数与重复查询，尚未做基准测量，不能保证单纯搬移加法就降低服务端负载。离线查看及权限变化后的缓存展示策略待确认。

依据：[metrics/service/MetricsService.java:14](/Users/bingbing.chen/mying/llm-platform/llm-platform-core/src/main/java/com/prosperllm/llmplatform/metrics/service/MetricsService.java:14)，类说明及 38–50 行分别调用 summary/series/modelDistribution/apiKeyRanking；[api/metrics/model/MetricsUsage.java:75](/Users/bingbing.chen/mying/llm-platform/llm-platform-api/src/main/java/com/prosperllm/llmplatform/api/metrics/model/MetricsUsage.java:75)，响应结构。

## 设计树与下一轮

- 已解决：首版统计范围为 Device Tool Usage；服务端为主数据源；四项汇总、按日趋势、详细分页请求日志及时间/工具/模型/状态筛选。仅查询当前账号、当前空间、当前设备曾接入工具的全部历史 Key；断开和换 Key 不移除历史，其他设备排除。
- 候选实现：服务端轻量时间桶＋客户端缓存合并；请求明细直接分页查询。
- 后续继续：企业空间权限；金额单位和 token/cache 公式；TTFT/TPS 是否进入首版；刷新与历史范围；离线缓存可见性。
- 不将 Current Workspace 与 Bound Workspace 混用：切换桌面空间不改变已有工具绑定；来自不同空间的请求不能静默拼入当前空间总额。

调研阶段未修改产品代码。后续用户确认继续后已本地实现，见 [桌面用量统计](desktop-usage.md)。首版查询边界和数据源可以独立演进，未单独创建 ADR。

### 本设备历史工具 Key 与分页明细（最新确认）

用户已明确上下布局：上方四项汇总与趋势，下方直接展示逐条请求日志并分页。汇总和趋势覆盖完整筛选范围，分页仅影响明细；三者必须使用同一组可查询 Key、时间及其他筛选条件。切换账号、空间、设备或筛选范围时废弃旧游标、旧响应及不再适用的缓存。

查询范围按当前登录账号、Current Workspace、当前设备的历史工具 Key 归属确定。CLI/桌面共享设备身份。同一工具在此范围创建过 K1、K2、K3，则三者的流水都计入，即使当前只使用 K3 或已经取消接入。另一设备的 K4、当前设备在其他空间的 K5 均排除。没有符合条件的历史 Key 时展示空态，不能回退到全空间查询。

服务端按持久化 Key 的用户、空间、设备、工具归属解析历史 Key 集合，并校验主体和空间权限；不根据可编辑 Key 名称判断归属。不能只读取本地当前连接或绑定表当前 Key，否则会遗漏已断开工具及同一工具的旧 Key。实现时核实轮换、删除后的历史元数据保留及关联，现有精确 Key 集合查询需扩展。

此前对“仅当前仍接入工具”的状态同步讨论已被用户最新范围取代，**不新增接入/取消接入状态同步来支持本页**。Disconnect 在本地恢复配置并保留 Key 的现有行为与展示历史用量相容。依据：[connections.rs:787](../rust/tokenmarket-core/src/connections.rs#L787-L873)。

设备范围实现注意：当前 desktop grant 保存 userId/clientId/有效期，未绑定 deviceId；现有 Key 操作中的 deviceId 来自请求参数。依据：[DesktopAuthRedis.java:55](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/desktop/redis/DesktopAuthRedis.java:55)、[ClientKeyWebService.java:24](/Users/bingbing.chen/mying/llm-platform/llm-platform-webapi/src/main/java/com/prosperllm/llmplatform/web/client/service/ClientKeyWebService.java:24)。本页必须按当前设备过滤并校验 Key 归属；设备参数与会话如何绑定是接口实现细节，不再与工具当前接入状态耦合。按设备签发的 Key 归属统计不等于对真实发起请求的物理设备做证明。
