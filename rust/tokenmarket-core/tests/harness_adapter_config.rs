use tokenmarket_core::{
    adapters::{get_adapter, AdapterContext},
    models::ToolConfig,
    ToolId,
};

#[test]
fn terminal_harness_setup_preserves_unrelated_yaml_numbers_as_scalars() {
    // Isolate the adapter's environment from other integration test processes.
    let root = tempfile::tempdir().unwrap();
    std::env::set_var("DSH_HOME", root.path());
    let path = root.path().join("cordis.patch.yml");
    std::fs::write(
        &path,
        "- id: ui-settings-account\n  config:\n    version: 1\n",
    )
    .unwrap();
    get_adapter(ToolId::DeepSeekHarness)
        .write_config(
            &ToolConfig {
                base_url: "https://example.test".into(),
                api_key: "sk-test-fixture".into(),
                model: "glm-5.3".into(),
                context_window_tokens: None,
            },
            &AdapterContext {
                setup_id: "fixture-setup".into(),
                key_id: "fixture-key".into(),
            },
        )
        .unwrap();
    let native: serde_yaml::Value = serde_yaml::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(native[0]["config"]["version"].as_u64(), Some(1));
    let providers = native[1]["config"]["providers"].as_mapping().unwrap();
    assert_eq!(providers.len(), 2);
    let glm = providers
        .values()
        .find(|p| p["models"][0]["id"].as_str() == Some("glm-5.3"))
        .unwrap();
    let ds = providers
        .values()
        .find(|p| p["models"][0]["id"].as_str() == Some("deepseek-v4.1-flash"))
        .unwrap();
    assert_eq!(glm["displayName"].as_str(), Some("Token Market:GLM-5.3"));
    assert_eq!(glm["reasoning"].as_str(), Some("high"));
    assert_eq!(ds["reasoning"].as_str(), Some("high"));
    let models = [&glm["models"][0], &ds["models"][0]];
    assert_eq!(models[0]["contextWindow"].as_u64(), Some(1_000_000));
    assert_eq!(models[0]["reasoningEfforts"]["max"].as_str(), Some("max"));
    assert!(models[0]["reasoningEfforts"].get("off").is_none());
    assert_eq!(
        models[1]["compat"]["thinkingFormat"].as_str(),
        Some("deepseek")
    );
    assert!(models[1]["reasoningEfforts"].get("off").unwrap().is_null());
}
