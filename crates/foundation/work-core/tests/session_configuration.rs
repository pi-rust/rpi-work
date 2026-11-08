//! Configuration observations preserve negotiated ids/values without inventing support.
use serde_json::{Value, json};
use work_core::api::SessionConfiguration;

#[test]
fn session_configuration_preserves_grouped_options_and_command_membership() {
    let value = json!({"protocol":"acp", "supportedCommands":[], "configOptions":[{
        "id":"opaque-id", "name":"Mode", "type":"select", "currentValue":"precise",
        "options":[{"name":"Group", "options":[{"value":"precise", "name":"Precise", "future":true}]}]
    }]});
    let configuration: SessionConfiguration = serde_json::from_value(value.clone()).unwrap();
    assert!(!configuration.supports_command("get_state"));
    assert!(!configuration.supports_command("set_active_tools"));
    assert_eq!(serde_json::to_value(configuration).unwrap(), value);
    let configuration = SessionConfiguration {
        protocol: "rpi".into(),
        supported_commands: vec!["get_state".into()],
        config_options: vec![],
    };
    assert!(configuration.supports_command("get_state"));
    assert!(!configuration.supports_command("state"));
}

#[test]
fn incomplete_observations_fail_instead_of_assuming_support() {
    for value in [
        Value::Null,
        json!({}),
        json!({"protocol":"rpi"}),
        json!({"protocol":"rpi", "supportedCommands":[]}),
    ] {
        assert!(serde_json::from_value::<SessionConfiguration>(value).is_err());
    }
}
