use serde_json::Value;
use tokenmarket_core::{
    adapters::{get_adapter, AdapterContext},
    models::ToolConfig,
    ToolId,
};

#[test]
fn terminal_workbuddy_uses_shared_capabilities_for_each_supported_model() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("models.json");
    std::env::set_var("WORKBUDDY_CONFIG_PATH", &path);
    for (id, name, off) in [
        ("glm-5.3", "GLM-5.3", false),
        ("deepseek-v4.1-flash", "DeepSeek V4.1 Flash", true),
    ] {
        std::fs::write(&path, r#"[{"id":"manual","name":"Keep"}]"#).unwrap();
        get_adapter(ToolId::WorkBuddy)
            .write_config(
                &ToolConfig {
                    base_url: "https://example.test".into(),
                    api_key: "sk-test-fixture".into(),
                    model: id.into(),
                    context_window_tokens: Some(128000),
                },
                &AdapterContext {
                    setup_id: "fixture".into(),
                    key_id: "fixture-key".into(),
                },
            )
            .unwrap();
        let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value[0]["name"], "Keep");
        assert_eq!(value[1]["id"], id);
        assert_eq!(value[1]["name"], format!("Token Market · {name}"));
        assert_eq!(value[1]["maxInputTokens"], 128000);
        assert!(value[1]["maxOutputTokens"].as_u64().unwrap() > 0);
        assert_eq!(value[1]["supportsReasoning"], true);
        assert_eq!(value[1]["reasoning"]["canDisableThinking"], off);
        assert_eq!(
            value[1]["reasoning"]["supportedEfforts"],
            serde_json::json!(["low", "high", "max"])
        );
    }
    std::env::remove_var("WORKBUDDY_CONFIG_PATH");
}
