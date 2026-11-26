use super::*;
use std::collections::HashMap;

#[test]
fn env_config_supports_unconfigured_loopback_and_lan_urls() {
    let empty = HashMap::<String, String>::new();
    assert_eq!(
        AiGatewayConfig::from_lookup(|name| empty.get(name).cloned()).expect("unconfigured"),
        AiGatewayConfig::Unconfigured
    );
    let mut values = HashMap::new();
    values.insert(
        "LOOM_GATEWAY_BASE_URL".to_owned(),
        "http://192.168.1.40:4200".to_owned(),
    );
    values.insert("LOOM_GATEWAY_MODEL".to_owned(), "qwen-home".to_owned());
    values.insert("LOOM_GATEWAY_TOKEN".to_owned(), "secret-token".to_owned());
    values.insert("LOOM_GATEWAY_TIMEOUT_SECS".to_owned(), "15".to_owned());
    let config =
        AiGatewayConfig::from_lookup(|name| values.get(name).cloned()).expect("lan config");
    assert!(matches!(config, AiGatewayConfig::Gateway(_)));
    assert!(!format!("{config:?}").contains("secret-token"));
    values.insert("LOOM_GATEWAY_TIMEOUT_SECS".to_owned(), "0".to_owned());
    assert_eq!(
        AiGatewayConfig::from_lookup(|name| values.get(name).cloned()).expect_err("zero timeout"),
        AiGatewayConfigError::TimeoutOutOfRange(0)
    );
}

#[test]
fn credential_read_and_clear_errors_do_not_report_success() {
    let root =
        std::env::temp_dir().join(format!("loom-gateway-credentials-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    // A directory in place of the private store is a portable read/delete failure.
    fs::create_dir(root.join("plugin-credentials.json")).unwrap();
    assert!(load_persisted_token(&root).is_err());
    let service = ManagedAiGateway::new(AiGatewayConfig::Unconfigured, None).unwrap();
    let update: AiGatewayConfigUpdate = serde_json::from_value(json!({
        "enabled": true, "baseUrl": "http://127.0.0.1:4200", "clearAuthToken": true
    }))
    .unwrap();
    assert!(service.update_config(update, &root).is_err());
    assert!(!service.status().configured);
    assert!(!settings_file_path(&root).exists());
    fs::remove_dir_all(root).unwrap();
}
