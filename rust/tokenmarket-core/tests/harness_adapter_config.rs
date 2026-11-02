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
}
