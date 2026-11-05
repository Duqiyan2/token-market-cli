# CC Switch 工具列表与安装管理调研

> 2026-11-04 后续决定：用户已明确撤销此前 npm 优先与固定安装版本策略；当前实现规范以 [工具生命周期](tool-lifecycle.md) 为准。下文保留为当时的调研记录。

调研日期：2026-11-04。上游确认是 [`farion1231/cc-switch`](https://github.com/farion1231/cc-switch)，本次固定到 main 的 [`e217bfcbbbbbf82f4619ab62f08fb002635b7858`](https://github.com/farion1231/cc-switch/commit/e217bfcbbbbbf82f4619ab62f08fb002635b7858)（2026-11-04 03:37:11 UTC）。以下“已实现”指该提交的代码行为；“建议”是对 Token Market 的推论。本次只读源码，没有安装、运行上游客户端，也没有修改产品代码。

## 结论

CC Switch 值得参考的是：把安装状态、当前版本、可执行入口、安装来源和升级目标关联起来；来源未知时不强行贴标签，升级也不随意另装一份 npm 版本。它并非全面隐藏安装方式：主列表展示已识别来源和路径，未安装时展示安装计划。它也并非统一 npm 优先：部分工具新装采用官方脚本优先、npm 兜底。Token Market 可以借用信息结构与目标校验，保留本项目已经确定的 npm 优先策略。[列表渲染](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/AppsPage.tsx#L715-L827)、[来源徽章](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/AppsPage.tsx#L97-L108)、[新装策略](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L4954-L5005)、[未知原生安装的升级处理](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L4883-L4939)。

## 已实现：列表如何呈现

- 13 个 CLI：Claude Code、Codex、Gemini CLI、Grok Build、OpenCode、OpenClaw、Hermes、Pi、MiniMax Code、Kimi Code、DeepSeek Harness、Antigravity CLI、Oh My Pi；另有 Claude Desktop 行。[工具枚举与名称](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/useToolManagement.ts#L41-L55)、[名称映射](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/useToolManagement.ts#L214-L249)、[Desktop 独立行](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/AppsPage.tsx#L338-L359)。
- 按“已安装／未安装”分组；首次探测未完成时暂不分组，避免逐行跳动。发现版本、发现但无法运行、安装位置报告非空，任一满足都算“本机存在”。Claude Desktop 使用独立状态，不套用 CLI 版本逻辑。[存在判定及分组](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/AppsPage.tsx#L145-L225)。

| 情况 | 主行内容与操作 |
| --- | --- |
| 已安装且可运行 | 当前版本；有更新则附新版本与“升级”；主信息下展示入口路径 |
| 未安装 | “未安装”与“安装”，下方是计划安装方式提示 |
| 已发现但无法运行 | “无法运行”与错误详情；不直接给安装／升级按钮；仍可进入卸载 |
| 多处安装 | 主行采用有默认标记的安装，否则采用报告第一项；展示“另有 N 处安装”，展开看各处来源、路径、版本和默认标记 |
| 新版本要求更高 Node | 展示运行时要求，暂不给升级操作 |

证据：[状态、按钮和主行路径](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/AppsPage.tsx#L715-L827)、[错误、Node 要求、展开列表和卸载菜单](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/AppsPage.tsx#L876-L967)、[单处安装行](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/settings/ToolInstallRow.tsx#L4-L43)。第一项兜底不能被解读成已经确认的终端默认入口。

来源徽章只展示 Homebrew、npm（由 nvm/Volta/fnm/mise 分类映射）、bun、pnpm、Scoop、pip；`system` 刻意不展示。不存在统一的“原生／官方脚本”徽章。实际分类主要由路径推断，Homebrew 额外检查解析软链后的真实文件位置；因此也不等同于完整、确定的历史安装记录。[前端来源映射](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/AppsPage.tsx#L97-L108)、[后端来源推断](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L3098-L3149)。

## 已实现：安装渠道、检测与升级

### 新装和已有安装使用不同规则

| 工具或情况 | 本次固定提交的实际策略 |
| --- | --- |
| POSIX Claude／Grok／OpenCode／MiniMax | 官方安装脚本，失败后 npm 兜底 |
| Windows Grok／MiniMax | 官方 PowerShell 安装脚本，失败后 npm 兜底 |
| Windows Claude／OpenCode；各平台 Codex／Gemini／OpenClaw／Pi／DeepSeek Harness | 对应 npm 包安装；DeepSeek Harness 附 `--engine-strict` |
| Hermes／Kimi／Antigravity／Oh My Pi | 官方安装脚本；具体工具采用各自升级逻辑 |
| 已有 Homebrew／Node 管理器安装 | 升级尽量写回现有安装：brew formula/cask、Volta、bun、同目录 npm 等 |
| 已识别官方安装布局 | 根据工具使用绝对路径 CLI 自升级，或固定安装目录后重跑官方安装器 |
| 无法识别渠道的原生可执行文件 | 若真实文件不在 `node_modules`，禁止自动退到 npm，提示使用原安装渠道 |

证据：[npm 安装表](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L906-L927)、[Windows 与特殊工具分支](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L995-L1084)、[POSIX 新装分支](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L4983-L5005)、[包管理器及官方安装升级](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L4007-L4254)、[未知原生安装保护](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L4899-L4939)。

“npm 分发”和“原生二进制”可以同时成立。该项目对 npm `node_modules` 内的原生程序保留包管理器升级可能，并为 Claude／OpenCode 等 npm 分发设置 optional dependencies、postinstall 所需参数；文件魔数用于判断可执行格式，不用于证明用户曾运行过哪个脚本。某些官方脚本还会管理隔离运行时或包目录，不能把“脚本安装”一律解释成“单文件原生下载”。[npm 脚本与平台二进制](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L930-L943)、[格式判定边界](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L4909-L4939)、[各安装器注释与命令](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L649-L689)。这些是 CC Switch 的实现认知，本次没有独立审计每家厂商脚本的全部实际行为。

### 检测和多处安装

本地版本优先探测终端 PATH 命中的工具，仅在未找到时扫描常见目录；有明确的执行错误时保留“已安装但失败”状态，避免让其他旧安装掩盖坏掉的默认安装。候选目录同时涵盖常见安装器、Node 管理器与 PATH。枚举用 canonicalize 去重，为每项单独运行 `--version`，并标记实际 PATH 默认入口。[版本入口](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L1242-L1279)、[常见目录与 PATH](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L2601-L2789)、[逐安装枚举](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L3384-L3475)。

最新版多从 npm 查询，部分工具有 GitHub／PyPI／专用接口路径；这和本机通过什么渠道安装是两个问题。安装或升级命令结束后再查当前版本；退出码为 0 但版本未变且仍落后时给警告，而非直接报升级成功。[最新版来源](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L1318-L1368)、[结果核验](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/useToolManagement.ts#L710-L788)。

升级默认目标只接受 PATH 默认项，或唯一一处安装；多处但找不到默认项则不假定第一项就是目标。至少两处安装就需要升级确认，即使版本相同；“冲突”则要求版本分歧或运行能力不一致。确认弹窗列出各处与将执行的命令，未锚定会提示；执行端在拿到生命周期锁之后重新探测、生成命令。[默认目标](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L4257-L4269)、[阈值与报告](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L5021-L5065)、[升级弹窗](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/settings/ToolUpgradeConfirmDialog.tsx#L26-L120)、[执行时重探测](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L334-L380)。注意：重新生成升级命令并不等于锁定用户确认时的同一份计划；不能据此声称上游完全消除了预览到执行间的目标变化。

卸载则明确让用户选择具体安装，预览与执行重新推导的计划必须一致；只删除识别到的程序文件或使用对应包管理器，保留配置和用户数据。未知安装转为原渠道处理。[卸载候选与计划](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/tool_uninstall.rs#L1-L149)、[卸载计划校验与结果核验](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/tool_uninstall.rs#L151-L242)。

## 对 Token Market 的建议（推论，尚未实施）

当前本地代码已经展示“安装来源＋入口路径＋另有 N 处”，详情也区分“安装方式／安装方案”，并有当前入口和实际文件。因此应精简与校准语义，无需照搬另一套列表。[本地主列表](../src/tauri-ui/tools.js#L76-L122)。本地检测先识别 Homebrew、应用内置和已验证 npm 包，再检查二进制魔数；最后输出的“原生程序”只证明格式，不能据此补写“官方脚本安装”。[本地来源检测](../rust/tokenmarket-core/src/installation/discovery.rs#L9-L92)。本地引用指 2026-11-04 的工作区快照，含未提交改动。

| 信息 | 建议呈现与依据 |
| --- | --- |
| 主列表 | 以工具名、安装／版本状态、接入状态和当前可执行操作为主；来源若保留，使用可信且简短的标签 |
| 未安装工具 | 明确写“安装方案：npm”等未来动作，不让计划值看起来像历史事实；由后端实际安装计划生成 |
| 已安装工具 | 已验证 npm 归属就写 npm；只有原生格式证据时主列表不贴渠道徽章，详情写“安装来源未确认”，将可执行格式单独说明，不推断脚本来源 |
| 详情／展开 | 保留当前入口、真实文件、渠道证据；多处安装时说明本客户端选择哪处，无法确认终端默认就明确说明 |
| 安装优先级 | 保留本项目 npm 优先决策；存在其他渠道时按既有升级能力和归属处理，避免因为“支持 npm”就另装一份 |
| 升级确认 | 展示这次具体目标路径与方式；多处安装重点说明影响哪处；预览与执行目标变化时重新确认 |
| 结果 | 用执行后的真实版本和可运行状态判断成功，不只依赖安装器退出码 |

需要避免照搬上游静态文案。该快照中文 `installHint` 把 Grok 写成 npm，但后端一键新装优先脚本；Claude／OpenCode 的提示没有区分 Windows 的 npm 分支；Kimi 和 DeepSeek Harness 还缺少对应提示。手动复制命令也存在 Grok 用 npm、Oh My Pi 用 bun，而一键安装采用官方脚本的差别。可借鉴它的状态结构，但渠道文案应来自同一份执行计划，避免三处维护后漂移。[中文提示](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/i18n/locales/zh.json#L4290-L4302)、[手动命令](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src/components/apps/useToolManagement.ts#L156-L208)、[实际安装分支](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L995-L1084)、[POSIX 一键安装](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L4983-L4998)。

## 边界

这是固定提交的静态代码调研，未验证发布包与该提交一致，未做 Windows／macOS／Linux 真机验收。来源路径规则可能误判或漏掉自定义布局；常见路径扫描也不能证明列出了机器上的全部安装。WSL 升级不使用 Windows 本机安装枚举进行绝对路径锚定，走独立 POSIX 路径，因此上述多处安装确认不能一概推及 WSL。[WSL 规划边界](https://github.com/farion1231/cc-switch/blob/e217bfcbbbbbf82f4619ab62f08fb002635b7858/src-tauri/src/commands/misc.rs#L5007-L5030)。本次仅整理调研与建议，未改动产品代码。
