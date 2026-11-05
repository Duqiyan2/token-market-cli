# 桌面用量统计

本地实现日期：2026-11-05。桌面侧新增“用量统计”页：上方四项汇总和按日趋势，下方逐条分页流水。后端需同时发布 Core 和 WebAPI 新接口后才能线上使用；本次没有部署或推送。

## 查询范围

当前登录账号、当前空间、本设备曾接入工具的全部历史 API Key。服务端从 `api_tokens` 按 user/workspace/device/source/tool 取 ID 集合，不用本地当前连接或 `client_key_bindings` 的当前 Key 推导历史。包含旧 Key、过期、停用、软删除 Key 和已断开的工具；不包含其他设备、其他空间、其他用户或第三方直连。历史范围依赖平台保留的 Key 归属元数据和请求日志。

CLI/桌面沿用同一个设备标识。设备标识由 Rust 读取，不由网页传入；服务端还校验桌面登录客户端、当前账号/空间和实时日志查看权限。此页按 Key 的签发设备归属统计；Key 被复制到别处使用时不会据此改变归属。

无历史 Key 返回空态，不能回退到全空间查询。取消接入无需向服务端同步状态。切换账号/空间和退出登录会清除页面数据、游标及内存缓存；迟到的旧响应会被丢弃。

## 页面与计算

- 时间：今天、近 7 天、近 30 天，默认近 30 天；日界和时间显示固定北京时间。
- 筛选：工具、模型名称、成功/失败/取消。翻页沿用已应用的筛选和时间截止点。
- 四项汇总：总费用、总请求数、平台记录的总 Token、缓存读取命中率。点击卡片切换趋势指标。
- 明细：时间、工具、模型、Key 名称及 ID 提示、费用、输入/输出/缓存读取、状态码、耗时、首字延迟。缺少延迟时显示“—”。
- 每页 20/50/100 条，默认 20；使用稳定游标前后翻页。翻页不重算汇总。
- 费用保留平台微额度，按整数累加、显示六位小数；不猜测 USD/CNY，也不按模型单价重算。与浏览器流水单位一致。

服务端一次返回同条件按日可加统计，客户端求和并绘图，不下载全量日志。总 Token 直接使用日志 `total_tokens`，不重复加缓存。缓存命中率为累计缓存读取 / 累计规范化输入；规范化输入逐条取 `max(total_tokens-output_tokens, input_tokens, 0)`，兼容缓存包含方式及旧 total 缺失的保守回退，分母为零显示“—”。不会对每天百分比取平均。

现有 metrics rollup 缺少缓存写入和日志结果等必要维度，因此首版日桶与明细使用同一个日志过滤条件；不要宣称日桶已经改为现有 metrics 预聚合表读取。查询最多跨 31 天，Core 对同条件同 Key 范围日桶缓存 30 秒（最多 512 项），客户端内存缓存 30 秒（最多 12 项）。刷新、筛选、进入页面/激活时按需读取，没有空闲轮询。尚未测量生产查询负载；未来可在保持契约下迁移到维度完整的预聚合数据源。

## 接口

`POST /api/client/usage/logs` 和 `POST /api/client/usage/buckets`，以 Client Session Bearer 鉴权。公共请求字段：

```json
{
  "workspaceId": "7",
  "deviceId": "native-device-id",
  "startTime": 1791129600000,
  "endTime": 1793620000000,
  "tool": "codex",
  "model": "",
  "status": "ALL",
  "cursor": null,
  "page": 1,
  "pageSize": 20
}
```

用户身份和允许工具集合由服务端确定；不接收任意用户或 Key ID 列表。响应包含 tools（工具及历史 Key 数）、buckets（日统计）或 items（分页流水），以及 nextCursor/page/pageSize/asOf。Key ID 和金额使用字符串，避免整数精度丢失。服务端只投影页面所需公开字段。

Rust 命令 `desktop_usage_logs` / `desktop_usage_buckets` 还校验请求的账号和空间，查询前后检查共享登录及当前空间。旧服务端返回 404 时页面明确提示服务未启用，不伪造零用量。

## 验证

已通过：

- `cargo check --locked --manifest-path src-tauri/Cargo.toml`，以及 Tauri `--lib` 的 3 项测试。
- `cargo test --locked --manifest-path rust/tokenmarket-core/Cargo.toml --test desktop_usage`：3 项。
- `node --test tests/desktop-usage.test.cjs tests/desktop-ui.test.cjs`：独立 main 功能快照 74 项通过；需要 Playwright，按 `desktop-ui.md` 配置外部运行时。
- 后端相关 ClientUsage、ApiLog DAO/逻辑/转换及 HTTP facade 测试：61 项通过（Core 42、WebAPI 19）；覆盖历史 Key 过滤、空 Key 集合不扩权、首次游标分页、桶不受明细游标影响、权限变化、客户端隔离及大整数响应。
- 使用隔离演示数据检查了 1280px 明暗主题、900px 页面宽度及横向表格滚动。

截图和 DOM 测试使用隔离 IPC，不能视为真实账号、部署或生产数据库验收。本次未运行真实 PostgreSQL 数据集查询，未做生产负载基准，也未发送付费模型请求。
