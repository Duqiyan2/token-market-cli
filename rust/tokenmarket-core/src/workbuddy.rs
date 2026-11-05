//! Project the shared catalogue into WorkBuddy's custom-model configuration.
use serde_json::{json, Value};

pub(crate) fn model_settings(model: &Value) -> Value {
    let model = crate::model_catalog::complete_metadata(model);
    // The connection selector admits TEXT_GENERATION models, our Agent models.
    // WorkBuddy renders custom models as name:id. Keep the provider label here
    // so the picker does not repeat the model ID already appended by WorkBuddy.
    let mut settings = json!({
        "name": "Token Market",
        "supportsToolCall": true,
    });
    let input = model["max_input_tokens"].as_u64().filter(|n| *n > 0);
    let context = model["context_window_tokens"].as_u64().filter(|n| *n > 0);
    // WorkBuddy has one input/context budget, not separate limits for both.
    if let Some(tokens) = match (input, context) {
        (Some(input), Some(context)) => Some(input.min(context)),
        (input, context) => input.or(context),
    } {
        settings["maxInputTokens"] = json!(tokens);
    }
    if let Some(tokens) = model["max_output_tokens"].as_u64().filter(|n| *n > 0) {
        settings["maxOutputTokens"] = json!(tokens);
    }
    if let Some(input) = model["input_modalities"]
        .as_array()
        .filter(|a| !a.is_empty())
    {
        settings["supportsImages"] = json!(input.iter().any(|v| v == "image"));
    }
    if let Some(temperature) = model["temperature"]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
    {
        settings["temperature"] = json!(temperature);
    }
    if let Some(supported) = model["supports_reasoning"].as_bool() {
        settings["supportsReasoning"] = json!(supported);
        let efforts = model["reasoning_efforts"].as_object();
        let can_disable = supported && efforts.is_some_and(|e| e.contains_key("off"));
        // Toggle-only and always-on models must not advertise effort control.
        let levels: Vec<_> = ["minimal", "low", "medium", "high", "xhigh", "max"]
            .into_iter()
            .filter(|level| {
                supported
                    && model["harness_compat"]["supportsReasoningEffort"] != false
                    && efforts.is_some_and(|e| e.contains_key(*level))
            })
            .collect();
        settings["onlyReasoning"] = json!(supported && !can_disable);
        settings["reasoning"] = json!({
            "supportedEfforts": levels,
            "canDisableThinking": can_disable,
        });
        // Prefer a balanced known tier; never default to an unsupported level.
        if let Some(default) = ["medium", "high", "low", "minimal", "xhigh", "max"]
            .into_iter()
            .find(|level| levels.contains(level))
        {
            settings["reasoning"]["defaultEffort"] = json!(default);
        }
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_lengths_modalities_and_distinct_reasoning_controls() {
        let glm = model_settings(
            &json!({"id":"glm-5.3", "max_input_tokens":128000, "max_output_tokens":8192, "input_modalities":["text","image"]}),
        );
        assert_eq!(glm["name"], "Token Market");
        assert_eq!(glm["maxInputTokens"], 128000);
        assert_eq!(glm["maxOutputTokens"], 8192);
        assert_eq!(glm["supportsImages"], true);
        assert_eq!(glm["onlyReasoning"], true);
        assert_eq!(
            glm["reasoning"],
            json!({"supportedEfforts":["low","high","max"], "defaultEffort":"high", "canDisableThinking":false})
        );
        let ds = model_settings(&json!({"id":"deepseek-v4.1-flash"}));
        assert_eq!(ds["maxInputTokens"], 1000000);
        assert_eq!(ds["onlyReasoning"], false);
        assert_eq!(ds["reasoning"]["canDisableThinking"], true);
        let kimi = model_settings(&json!({"id":"kimi-k2.6"}));
        assert_eq!(
            kimi["reasoning"],
            json!({"supportedEfforts":[], "canDisableThinking":true})
        );
        let minimax = model_settings(&json!({"id":"minimax-m2.5"}));
        assert_eq!(minimax["onlyReasoning"], true);
        assert_eq!(
            minimax["reasoning"],
            json!({"supportedEfforts":[], "canDisableThinking":false})
        );
    }

    #[test]
    fn preserves_unknowns_and_respects_explicit_disabled_reasoning() {
        assert_eq!(
            model_settings(&json!({"id":"custom"})),
            json!({"name":"Token Market", "supportsToolCall":true})
        );
        let disabled = model_settings(
            &json!({"id":"glm-5.3", "supports_reasoning":false, "max_input_tokens":2000000, "temperature":0}),
        );
        assert_eq!(disabled["supportsReasoning"], false);
        assert_eq!(disabled["onlyReasoning"], false);
        assert_eq!(disabled["maxInputTokens"], 1000000);
        assert_eq!(disabled["temperature"], 0.0);
        assert_eq!(
            disabled["reasoning"],
            json!({"supportedEfforts":[], "canDisableThinking":false})
        );
    }
}
