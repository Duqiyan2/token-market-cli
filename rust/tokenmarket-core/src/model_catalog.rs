//! Offline fallback metadata, never a source of model availability or credentials.
use serde_json::{json, Value};
use std::sync::OnceLock;

fn catalogue() -> &'static Value {
    static CATALOGUE: OnceLock<Value> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        serde_json::from_str(include_str!("model_capabilities.json"))
            .expect("bundled model capability snapshot is valid JSON")
    })
}

fn lookup(id: &str) -> Option<&'static Value> {
    let normalized = id.trim().to_ascii_lowercase();
    let models = &catalogue()["models"];
    if let Some(model) = models.get(&normalized) {
        return Some(model);
    }
    // Only known vendor namespaces are aliases. Do not guess capabilities for
    // arbitrary customer routes, model families, suffixes, or future versions.
    let (vendor, name) = normalized.split_once('/')?;
    if [
        "openai",
        "anthropic",
        "google",
        "deepseek",
        "deepseek-ai",
        "z-ai",
        "zai-org",
        "moonshotai",
        "minimax",
        "qwen",
        "x-ai",
        "xiaomi",
        "stepfun",
        "bytedance-seed",
        "mistralai",
    ]
    .contains(&vendor)
    {
        models.get(name)
    } else {
        None
    }
}

fn positive(value: &Value) -> bool {
    value.as_u64().is_some_and(|n| n > 0)
}

fn efforts(value: &Value) -> bool {
    value == false
        || value.as_object().is_some_and(|map| {
            !map.is_empty()
                && map.keys().any(|level| level != "off")
                && map.iter().all(|(level, wire)| {
                    ["off", "minimal", "low", "medium", "high", "xhigh", "max"]
                        .contains(&level.as_str())
                        && ((level == "off" && wire.is_null())
                            || wire.as_str().is_some_and(|s| !s.is_empty()))
                })
        })
}

/// Supplement only missing/invalid fields on an already authorized model.
/// Valid server values (including false) always win, field by field.
pub(crate) fn complete_metadata(model: &Value) -> Value {
    let mut result = model.clone();
    let Some(defaults) = model["id"].as_str().and_then(lookup) else {
        return result;
    };
    for field in [
        "context_window_tokens",
        "max_input_tokens",
        "max_output_tokens",
    ] {
        if !positive(&result[field]) && positive(&defaults[field]) {
            result[field] = defaults[field].clone();
        }
    }
    if result["display_name"]
        .as_str()
        .is_none_or(|s| s.trim().is_empty())
    {
        result["display_name"] = defaults["display_name"].clone();
    }
    if !result["input_modalities"]
        .as_array()
        .is_some_and(|items| !items.is_empty() && items.iter().all(Value::is_string))
    {
        result["input_modalities"] = defaults["input_modalities"].clone();
    }
    if !result["supports_reasoning"].is_boolean() {
        result["supports_reasoning"] = if efforts(&result["reasoning_efforts"]) {
            json!(result["reasoning_efforts"] != false)
        } else {
            defaults["supports_reasoning"].clone()
        };
    }
    if result["supports_reasoning"] == false || result["reasoning_efforts"] == false {
        result["supports_reasoning"] = json!(false);
        result["reasoning_efforts"] = json!(false);
    } else if !efforts(&result["reasoning_efforts"])
        && defaults["supports_reasoning"] == true
        && efforts(&defaults["reasoning_efforts"])
    {
        result["reasoning_efforts"] = defaults["reasoning_efforts"].clone();
    }
    if result["supports_reasoning"] == true {
        if let Some(default_compat) = defaults["harness_compat"].as_object() {
            if !result["harness_compat"].is_object() {
                result["harness_compat"] = json!({});
            }
            for (name, value) in default_compat {
                result["harness_compat"]
                    .as_object_mut()
                    .unwrap()
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
        }
    }
    result
}

/// Harness' explicit maxTokens also becomes its default request output budget.
/// Keep room for input and compaction, while the snapshot retains the real limit.
pub(crate) fn harness_model(model: &Value) -> Value {
    let model = complete_metadata(model);
    let mut row = json!({"id":model["id"]});
    if let Some(name) = model["display_name"].as_str().filter(|s| !s.is_empty()) {
        row["name"] = json!(name);
    }
    for (source, target) in [
        ("context_window_tokens", "contextWindow"),
        ("max_output_tokens", "maxTokens"),
    ] {
        if positive(&model[source]) {
            row[target] = model[source].clone();
        }
    }
    if let Some(max) = model["harness_max_tokens"].as_u64().filter(|n| *n > 0) {
        row["maxTokens"] = json!(max.min(model["max_output_tokens"].as_u64().unwrap_or(max)));
    } else if let (Some(output), Some(window)) =
        (row["maxTokens"].as_u64(), row["contextWindow"].as_u64())
    {
        row["maxTokens"] = json!(output.min(65_536).min((window / 4).max(1)));
    }
    if let Some(input) = model["input_modalities"].as_array() {
        let known: Vec<_> = input
            .iter()
            .filter(|v| matches!(v.as_str(), Some("text" | "image")))
            .collect();
        if !known.is_empty() {
            row["input"] = json!(known);
        }
    }
    if efforts(&model["reasoning_efforts"]) {
        row["reasoningEfforts"] = model["reasoning_efforts"].clone();
    } else if model["supports_reasoning"] == false {
        row["reasoningEfforts"] = json!(false);
    }
    if model["supports_reasoning"] != false && model["harness_compat"].is_object() {
        row["compat"] = model["harness_compat"].clone();
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_valid_and_covers_mainstream_models() {
        let models = catalogue()["models"].as_object().unwrap();
        assert!(models.len() >= 150);
        for (id, model) in models {
            assert!(positive(&model["context_window_tokens"]), "{id}");
            assert!(positive(&model["max_output_tokens"]), "{id}");
            if let Some(value) = model.get("reasoning_efforts") {
                assert!(efforts(value), "{id}");
            }
        }
        for id in [
            "deepseek-v4.1-flash",
            "glm-5.3",
            "kimi-k3",
            "minimax-m3",
            "qwen3.8-max",
            "gpt-6.1-sol",
            "claude-opus-4-8",
            "gemini-3.8-flash",
            "grok-4.7",
        ] {
            assert!(lookup(id).is_some(), "{id}");
        }
    }

    #[test]
    fn fallback_respects_server_fields_and_preserves_identity() {
        let server = json!({"id":"GLM-5.3", "display_name":"Custom", "context_window_tokens":200000, "max_output_tokens":4000, "supports_reasoning":false, "input_modalities":["text"], "custom":"keep"});
        let complete = complete_metadata(&server);
        for (name, value) in server.as_object().unwrap() {
            assert_eq!(&complete[name], value);
        }
        assert_eq!(complete["reasoning_efforts"], false);
        assert!(harness_model(&server).get("compat").is_none());
        let disabled = complete_metadata(&json!({"id":"glm-5.3", "reasoning_efforts":false}));
        assert_eq!(disabled["supports_reasoning"], false);
    }

    #[test]
    fn known_namespaces_resolve_but_unknown_models_are_untouched() {
        assert!(lookup("deepseek-ai/DeepSeek-V4.1-Flash").is_some());
        for id in [
            "customer/glm-5.3",
            "glm-5.3-custom",
            "glm-999",
            "deepseek-v4.1-flash-unknown",
        ] {
            let model = json!({"id":id});
            assert_eq!(complete_metadata(&model), model);
        }
    }

    #[test]
    fn reasoning_and_output_budgets_match_exact_models() {
        let glm = harness_model(&json!({"id":"glm-5.3"}));
        assert_eq!(
            glm["reasoningEfforts"],
            json!({"low":"low", "high":"high", "max":"max"})
        );
        assert_eq!(glm["compat"]["thinkingFormat"], "openai");
        assert_eq!(glm["contextWindow"], 1_000_000);
        assert_eq!(glm["maxTokens"], 65_536);
        let ds = harness_model(&json!({"id":"deepseek-v4.1-flash"}));
        assert!(ds["reasoningEfforts"].get("off").unwrap().is_null());
        assert_eq!(ds["compat"]["thinkingFormat"], "deepseek");
        let kimi = harness_model(&json!({"id":"kimi-k2.6"}));
        assert_eq!(kimi["reasoningEfforts"], json!({"off":null,"high":"high"}));
        assert_eq!(kimi["compat"]["supportsReasoningEffort"], false);
        let fixed = harness_model(&json!({"id":"MiniMax-M2.5"}));
        assert_eq!(fixed["reasoningEfforts"], json!({"high":"high"}));
        let limited = harness_model(&json!({"id":"glm-5.3", "max_output_tokens":8192}));
        assert_eq!(limited["maxTokens"], 8192);
    }
}
