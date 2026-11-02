//! YAML output must not expose serde_json's arbitrary-precision number wrapper.
use anyhow::{Context, Result};
use serde::Serialize;

pub(crate) fn to_string(value: &impl Serialize) -> Result<String> {
    // JSON is valid YAML. Parsing the JSON representation into native YAML
    // values preserves scalar types without forwarding serde_json's private
    // Serialize implementation for numbers to serde_yaml.
    let json = serde_json::to_string(value)?;
    let native: serde_yaml::Value =
        serde_yaml::from_str(&json).context("配置包含无法写入 YAML 的值")?;
    Ok(serde_yaml::to_string(&native)?)
}

#[cfg(test)]
mod tests {
    use super::to_string;
    use serde_json::json;

    #[test]
    fn writes_native_yaml_scalars_in_nested_documents() {
        let value = json!({
            "version": 1,
            "models": [{"contextWindow": 65536, "cost": 0.25}],
            "signed": i64::MIN,
            "unsigned": u64::MAX,
            "enabled": true,
            "missing": null,
            "strings": ["1", "true", "null", "中文\nvalue"],
        });
        let output = to_string(&value).unwrap();
        assert!(!output.contains("$serde_json::private::Number"));
        let native: serde_yaml::Value = serde_yaml::from_str(&output).unwrap();
        assert_eq!(native["version"].as_u64(), Some(1));
        assert_eq!(native["models"][0]["contextWindow"].as_u64(), Some(65536));
        assert_eq!(native["models"][0]["cost"].as_f64(), Some(0.25));
        assert_eq!(native["signed"].as_i64(), Some(i64::MIN));
        assert_eq!(native["unsigned"].as_u64(), Some(u64::MAX));
        assert_eq!(
            serde_yaml::from_str::<serde_json::Value>(&output).unwrap(),
            value
        );
    }
}
