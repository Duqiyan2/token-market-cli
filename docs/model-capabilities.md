# 内置模型能力目录

App 和 CLI 编译时嵌入 `rust/tokenmarket-core/src/model_capabilities.json`，无需额外请求能力接口。
当前快照覆盖 200 个模型及版本，包括 DeepSeek、GLM、Kimi、MiniMax、Qwen、GPT、Claude、Gemini、Grok、MiMo、Mistral、Step 和 Seed。

## 接入规则

- `/v1/models` 仍决定当前 Key 可用的模型及 `TEXT_GENERATION` 类型，内置目录不增加模型权限。列表获取失败时保持原配置。
- 对已返回的模型补齐显示名、上下文窗口、最大输出、输入模态和已知推理能力。有效的服务端字段优先；缺失、null、零长度等无效容量使用内置值。
- 模型 ID 保持原样。匹配忽略大小写，并接受明确列出的厂商命名空间；不按宽泛家族前缀猜测未来版本或自定义路由。
- 补全后的元数据交给各工具现有适配器，按工具支持的字段写入。断开连接继续使用现有备份和逐字段恢复流程。
- 服务端与内置目录均不认识的模型保持现有行为，不虚构推理档位或容量。

## DeepSeek Harness

接入和重新接入时，逐模型写入 `contextWindow`、`maxTokens`、`input`、已确认的 `reasoningEfforts` 和 `compat`。CLI 的 Harness 配置也使用同一份投影。

| 示例 | 推理选项 |
| --- | --- |
| DeepSeek V4.1 Flash | Off / Low / High / Max |
| GLM-5.3 | Low / High / Max；不提供 Off |
| Kimi K2.6 | Off / High（开关，不发送强度参数） |
| Kimi K2.7 Code | High（常开，不发送强度参数） |
| MiniMax M2 系列 | High（常开，不声明可关闭或多档强度） |

不同模型分别使用 DeepSeek、Z.ai、Qwen 或标准 OpenAI 的请求格式。只从 OpenRouter 获取的补充条目当前补齐容量和模态，不照搬 OpenRouter 独有的推理控制。需要手动 thinking 预算的旧 Claude 条目也暂不生成推理选择器，避免将原生协议能力误报为 Chat 兼容路由能力；有明确的请求映射后再扩充。

GLM-5.3、Qwen3.8 等不支持关闭推理的模型使用网关标准 `reasoning_effort` 字段，由网关转换到原生协议。这样未选择强度时沿用上游默认值，避免 pi-ai 的 Z.ai/Qwen 格式因参数缺省而主动发送关闭指令。Claude 仅在网关已知的 adaptive 映射或常开模型上生成推理控制。

目录的 `max_output_tokens` 保留模型真实上限；Harness 的显式 `maxTokens` 同时是默认请求输出预算，会参与自动压缩的输入预留。因此默认写入 `min(模型最大输出, 65536, 上下文窗口 / 4)`，为输入留出空间。服务端可用 `harness_max_tokens` 明确指定请求预算（仍不超过模型输出上限）。用户也可以在 Harness 的模型高级设置中调整；再次接入会按当前目录重新生成托管配置。

所有长度单位均为 token。`input` 是文本/图片模态，不是输入长度。Harness 模型配置没有独立 `maxInputTokens`，本次不添加不存在的输入长度字段；系统提示、工具、历史和输出共享上下文窗口。小上下文模型仍需在 Harness 中配置适合窗口的压缩余量。

## 来源与维护

快照提取自本机 DeepSeek Harness `0.2.1-rc.5` 随附的 `@earendil-works/pi-ai` `1.1.0` 模型目录，核查日期为 2026-11-05。来源 provider 保存在各条目的 `source_provider` 中，MIT 授权保存在快照及 [授权文件](../scripts/licenses/pi-ai-MIT.txt) 中。

- [pi-ai 源码与模型目录](https://github.com/earendil-works/pi/tree/main/packages/ai)
- [Harness 自定义模型和推理配置](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/llm/llm-pi-ai/README.zh.md)
- [GLM-5.3 官方参数](https://docs.z.ai/guides/llm/glm-5.3)
- [DeepSeek 官方模型参数](https://api-docs.deepseek.com/quick_start/pricing/)

更新时使用解包后的 pi-ai npm 包目录，检查生成差异并运行 Rust 测试。脚本不读取 Key，不调用模型接口：

```sh
python3 scripts/update-model-capabilities.py /path/to/pi-ai --verified-on YYYY-MM-DD
cargo test --manifest-path rust/tokenmarket-core/Cargo.toml --all-targets --locked
```

更换网关渠道后仍须以实际请求验证推理和工具调用。内置能力是配置依据，不代表所有渠道、全部模型已完成真实调用验收。
