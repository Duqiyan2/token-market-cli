//! Tool-specific views of the authorized, completed model catalogue.
//! Never forward one tool's schema or arbitrary server fields to another tool.
use crate::{model_catalog, ToolId};
use serde_json::{json, Value};

pub(crate) const LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

pub(crate) fn effort_levels(model: &Value) -> Vec<&'static str> {
    if model["supports_reasoning"] != true
        || model["harness_compat"]["supportsReasoningEffort"] == false
        || !model_catalog::efforts(&model["reasoning_efforts"])
    {
        return vec![];
    }
    LEVELS[1..]
        .iter()
        .copied()
        .filter(|level| model["reasoning_efforts"].get(*level).is_some())
        .collect()
}

pub(crate) fn default_effort(model: &Value) -> Option<&str> {
    let levels = effort_levels(model);
    ["medium", "high", "low", "minimal", "xhigh", "max"]
        .into_iter()
        .find(|level| levels.contains(level))
        .and_then(|level| model["reasoning_efforts"][level].as_str())
}

pub(crate) fn thinking_mode(model: &Value) -> &'static str {
    if model["supports_reasoning"] == false {
        return "never";
    }
    if model["supports_reasoning"] != true || !model_catalog::efforts(&model["reasoning_efforts"]) {
        return "unknown";
    }
    if model["reasoning_efforts"].get("off").is_some() {
        "switchable"
    } else {
        "always"
    }
}

fn lengths(model: &Value, target: &mut Value, names: &[(&str, &str)]) {
    for (source, field) in names {
        if let Some(tokens) = model[*source].as_u64().filter(|n| *n > 0) {
            target[*field] = json!(tokens);
        }
    }
}

/// Pi and OpenClaw use null for *unsupported*, whereas Harness uses a null
/// off value for a native toggle. A string enables the off level; these native
/// dialects send their own boolean/type toggle instead of that string.
fn thinking_map(model: &Value) -> Option<Value> {
    if !model["reasoning_efforts"].is_object()
        || !model_catalog::efforts(&model["reasoning_efforts"])
    {
        return None;
    }
    let mut map = json!({});
    for level in LEVELS {
        match model["reasoning_efforts"].get(level) {
            Some(Value::Null) if level == "off" => {
                if matches!(
                    model["harness_compat"]["thinkingFormat"].as_str(),
                    Some("deepseek" | "zai" | "qwen")
                ) {
                    map[level] = json!("none");
                }
            }
            Some(value) => map[level] = value.clone(),
            None => map[level] = Value::Null,
        }
    }
    Some(map)
}

pub(crate) fn settings(tool: ToolId, model: &Value) -> Value {
    let model = model_catalog::complete_metadata(model);
    let id = model["id"].as_str().unwrap_or_default();
    let mut row = json!({"name": model["display_name"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or(id)});
    match tool {
        ToolId::Codex => {
            row = json!({});
            if let Some(window) = model["context_window_tokens"]
                .as_u64()
                .filter(|n| *n > 0 && *n <= i64::MAX as u64)
            {
                row["model_context_window"] = json!(window);
            }
            if let Some(effort) = default_effort(&model) {
                row["model_reasoning_effort"] = json!(effort);
            }
        }
        ToolId::ClaudeCode => {
            row = json!({});
            if let Some(supported) = model["supports_reasoning"].as_bool() {
                row["alwaysThinkingEnabled"] = json!(supported);
                let mut caps = vec![];
                if supported {
                    caps.push("thinking");
                }
                let levels = effort_levels(&model);
                if !levels.is_empty() {
                    caps.push("effort");
                }
                if levels.contains(&"xhigh") {
                    caps.push("xhigh_effort");
                }
                if levels.contains(&"max") {
                    caps.push("max_effort");
                }
                for family in ["FABLE", "HAIKU", "OPUS", "SONNET"] {
                    row["env"]
                        [format!("ANTHROPIC_DEFAULT_{family}_MODEL_SUPPORTED_CAPABILITIES")] =
                        json!(caps.join(","));
                }
                if let Some(level) = ["medium", "high", "low", "xhigh"]
                    .into_iter()
                    .find(|l| levels.contains(l))
                {
                    row["effortLevel"] = json!(level);
                }
            }
            if let Some(output) = model["max_output_tokens"].as_u64().filter(|n| *n > 0) {
                row["env"]["CLAUDE_CODE_MAX_OUTPUT_TOKENS"] = json!(output.min(64_000).to_string());
            }
            if let Some(window) = model["context_window_tokens"].as_u64().filter(|n| *n > 0) {
                row["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"] = json!(window.to_string());
            }
        }
        ToolId::Pi | ToolId::OpenClaw | ToolId::OhMyPi => {
            if tool == ToolId::OhMyPi {
                row["supportsTools"] = json!(true);
            }
            if tool == ToolId::OpenClaw {
                row["compat"]["supportsTools"] = json!(true);
            }
            lengths(
                &model,
                &mut row,
                &[
                    ("context_window_tokens", "contextWindow"),
                    ("max_output_tokens", "maxTokens"),
                ],
            );
            if let Some(input) = model["input_modalities"].as_array() {
                let input: Vec<_> = input
                    .iter()
                    .filter(|v| matches!(v.as_str(), Some("text" | "image")))
                    .collect();
                if !input.is_empty() {
                    row["input"] = json!(input);
                }
            }
            if let Some(reasoning) = model["supports_reasoning"].as_bool() {
                row["reasoning"] = json!(reasoning);
            }
            if tool != ToolId::OhMyPi {
                if let Some(map) = thinking_map(&model) {
                    row["thinkingLevelMap"] = map;
                }
                if model["supports_reasoning"] != false {
                    if let Some(compat) = model["harness_compat"].as_object() {
                        for (key, value) in compat {
                            // Shared, source-verified Pi/OpenClaw compatibility fields.
                            if [
                                "supportsStore",
                                "supportsDeveloperRole",
                                "maxTokensField",
                                "thinkingFormat",
                                "supportsReasoningEffort",
                                "requiresReasoningContentOnAssistantMessages",
                            ]
                            .contains(&key.as_str())
                            {
                                if !row["compat"].is_object() {
                                    row["compat"] = json!({});
                                }
                                row["compat"][key] = value.clone();
                            }
                        }
                    }
                }
            } else {
                let levels: Vec<_> = LEVELS[1..]
                    .iter()
                    .copied()
                    .filter(|level| model["reasoning_efforts"].get(*level).is_some())
                    .collect();
                if model["supports_reasoning"] == true && !levels.is_empty() {
                    row["thinking"] = json!({"mode":"effort", "efforts":levels, "requiresEffort":thinking_mode(&model) != "switchable"});
                    for level in &levels {
                        row["thinking"]["effortMap"][*level] =
                            model["reasoning_efforts"][*level].clone();
                    }
                    let preferred = ["medium", "high", "low", "minimal", "xhigh", "max"]
                        .into_iter()
                        .find(|l| levels.contains(l))
                        .unwrap();
                    row["thinking"]["defaultLevel"] = json!(preferred);
                }
                for key in [
                    "supportsStore",
                    "supportsDeveloperRole",
                    "maxTokensField",
                    "supportsReasoningEffort",
                ] {
                    if let Some(value) = model["harness_compat"].get(key) {
                        row["compat"][key] = value.clone();
                    }
                }
                if let Some(format) = model["harness_compat"]["thinkingFormat"].as_str() {
                    // OMP's zai dialect has the same thinking.type toggle as DeepSeek.
                    row["compat"]["thinkingFormat"] =
                        json!(if format == "deepseek" { "zai" } else { format });
                }
                if let Some(off) = model["reasoning_efforts"]["off"].as_str() {
                    if off == "none" {
                        row["compat"]["reasoningDisableMode"] = json!("none-effort");
                    }
                }
                if model["reasoning_efforts"]
                    .get("off")
                    .is_some_and(Value::is_null)
                {
                    match model["harness_compat"]["thinkingFormat"].as_str() {
                        Some("deepseek" | "zai") => {
                            row["compat"]["reasoningDisableMode"] = json!("zai-thinking-disabled")
                        }
                        Some("qwen") => {
                            row["compat"]["reasoningDisableMode"] =
                                json!("qwen-enable-thinking-false")
                        }
                        _ => {}
                    }
                }
                if model["harness_compat"]["requiresReasoningContentOnAssistantMessages"] == true {
                    row["compat"]["requiresReasoningContentForToolCalls"] = json!(true);
                }
            }
        }
        ToolId::OpenCode | ToolId::MiniMaxCode => {
            row["tool_call"] = json!(true);
            for (source, field) in [
                ("context_window_tokens", "context"),
                ("max_input_tokens", "input"),
                ("max_output_tokens", "output"),
            ] {
                if let Some(n) = model[source]
                    .as_u64()
                    .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
                {
                    row["limit"][field] = json!(n);
                }
            }
            if row["limit"].is_null() {
                row.as_object_mut().unwrap().remove("limit");
            }
            if let Some(input) = model["input_modalities"]
                .as_array()
                .filter(|a| !a.is_empty())
            {
                let input: Vec<_> = input
                    .iter()
                    .filter(|v| {
                        matches!(
                            v.as_str(),
                            Some("text" | "image" | "audio" | "video" | "pdf")
                        )
                    })
                    .collect();
                if !input.is_empty() {
                    row["modalities"] = json!({"input":input, "output":["text"]});
                    row["attachment"] = json!(input.iter().any(|v| *v != "text"));
                    if tool == ToolId::MiniMaxCode {
                        row["capabilities"]["support_image"] =
                            json!(input.iter().any(|v| **v == "image"));
                        row["capabilities"]["support_video"] =
                            json!(input.iter().any(|v| **v == "video"));
                    }
                }
            }
            if let Some(reasoning) = model["supports_reasoning"].as_bool() {
                row["reasoning"] = json!(reasoning);
            }
            let levels = effort_levels(&model);
            if tool == ToolId::OpenCode {
                // Overrides V1's family-based auto-generated tiers, including on/off-only models.
                row["reasoning_options"] = json!([]);
                if !levels.is_empty() {
                    let mut values: Vec<Value> = levels
                        .iter()
                        .map(|level| model["reasoning_efforts"][*level].clone())
                        .collect();
                    if let Some(off) = model["reasoning_efforts"]["off"].as_str() {
                        values.insert(0, json!(off));
                    }
                    row["reasoning_options"] = json!([{"type":"effort","values":values}]);
                }
            } else {
                if !levels.is_empty() {
                    row["thinking"] = json!({"effortOptions": levels.iter().map(|level| model["reasoning_efforts"][*level].clone()).collect::<Vec<_>>()});
                    if let Some(default) = default_effort(&model) {
                        row["thinking"]["defaultEffort"] = json!(default);
                    }
                }
                match thinking_mode(&model) {
                    "never" => row["thinking_config"] = json!({"mode":"forced_off"}),
                    "always" => row["thinking_config"] = json!({"mode":"forced_on"}),
                    "switchable" => row["thinking_config"] = json!({"mode":"switchable"}),
                    _ => {}
                }
            }
            if model["harness_compat"]["requiresReasoningContentOnAssistantMessages"] == true {
                row["interleaved"] = json!({"field":"reasoning_content"});
            }
        }
        _ => {}
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_null_means_unsupported_and_native_off_stays_available() {
        for tool in [ToolId::Pi, ToolId::OpenClaw] {
            let glm = settings(tool, &json!({"id":"glm-5.3"}));
            assert_eq!(glm["reasoning"], true);
            assert_eq!(
                glm["thinkingLevelMap"],
                json!({"off":null,"minimal":null,"low":"low","medium":null,"high":"high","xhigh":null,"max":"max"})
            );
            let ds = settings(tool, &json!({"id":"deepseek-v4.1-flash"}));
            assert_eq!(ds["thinkingLevelMap"]["off"], "none");
            assert_eq!(ds["compat"]["thinkingFormat"], "deepseek");
            let gpt = settings(tool, &json!({"id":"gpt-5.2"}));
            assert_eq!(gpt["thinkingLevelMap"]["off"], "none");
            let kimi = settings(tool, &json!({"id":"kimi-k2.6"}));
            assert_eq!(kimi["compat"]["supportsReasoningEffort"], false);
            assert_eq!(kimi["thinkingLevelMap"]["low"], Value::Null);
        }
    }

    #[test]
    fn server_only_models_get_their_own_controls_and_false_wins() {
        let model = json!({"id":"customer/route", "context_window_tokens":90000,"max_input_tokens":60000,"max_output_tokens":9000,"input_modalities":["text","image"],"reasoning_efforts":{"low":"low","max":"max"}});
        let complete = model_catalog::complete_metadata(&model);
        assert_eq!(complete["supports_reasoning"], true);
        let oc = settings(ToolId::OpenCode, &model);
        assert_eq!(
            oc["limit"],
            json!({"context":90000,"input":60000,"output":9000})
        );
        assert_eq!(
            oc["reasoning_options"],
            json!([{"type":"effort","values":["low","max"]}])
        );
        assert_eq!(
            oc["modalities"],
            json!({"input":["text","image"],"output":["text"]})
        );
        assert_eq!(
            settings(ToolId::Codex, &model)["model_reasoning_effort"],
            "low"
        );
        for tool in [
            ToolId::Pi,
            ToolId::OpenClaw,
            ToolId::OhMyPi,
            ToolId::OpenCode,
            ToolId::MiniMaxCode,
        ] {
            let disabled = settings(tool, &json!({"id":"glm-5.3","supports_reasoning":false}));
            assert_eq!(disabled["reasoning"], false, "{tool}");
            assert!(disabled.get("thinkingLevelMap").is_none());
            assert!(disabled.get("thinking").is_none());
        }
        let unknown = settings(ToolId::Pi, &json!({"id":"unknown"}));
        assert_eq!(unknown, json!({"name":"unknown"}));
    }

    #[test]
    fn toggle_only_and_always_on_models_never_gain_fabricated_efforts() {
        let kimi = settings(ToolId::MiniMaxCode, &json!({"id":"kimi-k2.6"}));
        assert_eq!(kimi["thinking_config"]["mode"], "switchable");
        assert!(kimi.get("thinking").is_none());
        let minimax = settings(ToolId::MiniMaxCode, &json!({"id":"minimax-m2.5"}));
        assert_eq!(minimax["thinking_config"]["mode"], "forced_on");
        assert!(minimax.get("thinking").is_none());
        for id in ["kimi-k2.6", "minimax-m2.5"] {
            assert_eq!(
                settings(ToolId::OpenCode, &json!({"id":id}))["reasoning_options"],
                json!([])
            );
        }
        let omp = settings(ToolId::OhMyPi, &json!({"id":"glm-5.3"}));
        assert_eq!(omp["thinking"]["efforts"], json!(["low", "high", "max"]));
        assert_eq!(omp["thinking"]["requiresEffort"], true);
        assert!(omp.get("thinkingLevelMap").is_none());
    }

    #[test]
    fn single_model_tools_use_known_limits_and_supported_defaults() {
        let model = json!({"id":"glm-5.3","context_window_tokens":200000,"max_output_tokens":8192});
        assert_eq!(
            settings(ToolId::Codex, &model),
            json!({"model_context_window":200000,"model_reasoning_effort":"high"})
        );
        let claude = settings(ToolId::ClaudeCode, &model);
        assert_eq!(claude["env"]["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "8192");
        assert_eq!(claude["effortLevel"], "high");
        assert_eq!(
            claude["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL_SUPPORTED_CAPABILITIES"],
            "thinking,effort,max_effort"
        );
        assert!(settings(ToolId::Codex, &json!({"id":"minimax-m2.5"}))
            .get("model_reasoning_effort")
            .is_none());
    }
}
