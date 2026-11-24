//! Decode typed operation receipts without inferring Agent acceptance/settlement.
use serde_json::Value;
use work_core::api::{
    HostResponse,
    realtime::{OperationOutcome, OperationStatus, RealtimeErrorCode},
};

/// `None` remains unknown; `Some(Err)` is a definite refusal. `Some(Ok)` only
/// confirms the original command result, never a run settlement or config effect.
/// Commands returning `HostResponse` must match their original command identity.
/// Queue/dialog acknowledgements additionally require an actual boolean value.
pub fn interpret_receipt(
    status: OperationStatus,
    expected_command: Option<&str>,
    requires_acceptance: bool,
) -> Option<Result<(), String>> {
    let value = match status {
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value },
        } => value,
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Failed { error },
        } if error.code != RealtimeErrorCode::OutcomeUnknown => return Some(Err(error.message)),
        _ => return None,
    };
    if expected_command.is_some() || value.get("type").and_then(Value::as_str) == Some("response") {
        let reply: HostResponse<Value> = serde_json::from_value(value).ok()?;
        if reply.response_type != "response"
            || expected_command.is_some_and(|command| reply.command != command)
        {
            return None;
        }
        let acceptance = reply
            .data
            .as_ref()
            .and_then(|data| data.get("accepted"))
            .and_then(Value::as_bool);
        let accepted = reply.success
            && if requires_acceptance {
                acceptance?
            } else {
                acceptance != Some(false)
            };
        return Some(if accepted {
            Ok(())
        } else {
            Err(reply.error.unwrap_or_else(|| "节点明确拒绝了原请求".into()))
        });
    }
    if requires_acceptance {
        return value
            .get("accepted")
            .and_then(Value::as_bool)
            .map(|accepted| {
                if accepted {
                    Ok(())
                } else {
                    Err("节点未受理原请求".into())
                }
            });
    }
    Some(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use work_core::api::realtime::RealtimeError;
    fn succeeded(value: Value) -> OperationStatus {
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value },
        }
    }
    fn host(success: bool, data: Value) -> Value {
        json!({"type":"response","id":null,"command":"prompt","success":success,"data":data,"error":if success { Value::Null } else { json!("refused") }})
    }
    #[test]
    fn pending_and_outcome_unknown_are_not_failures_or_successes() {
        for status in [
            OperationStatus::Reserved,
            OperationStatus::Dispatching,
            OperationStatus::OutcomeUnknown,
            OperationStatus::KnownResult {
                outcome: OperationOutcome::Failed {
                    error: RealtimeError {
                        code: RealtimeErrorCode::OutcomeUnknown,
                        message: "unknown".into(),
                    },
                },
            },
        ] {
            assert_eq!(interpret_receipt(status, Some("prompt"), false), None);
        }
    }
    #[test]
    fn known_failure_matches_the_sdk_rule_even_for_internal_error() {
        for code in [
            RealtimeErrorCode::Conflict,
            RealtimeErrorCode::Internal,
            RealtimeErrorCode::AccessDenied,
        ] {
            assert_eq!(
                interpret_receipt(
                    OperationStatus::KnownResult {
                        outcome: OperationOutcome::Failed {
                            error: RealtimeError {
                                code,
                                message: "refused".into()
                            }
                        }
                    },
                    None,
                    false
                ),
                Some(Err("refused".into()))
            );
        }
    }
    #[test]
    fn queue_host_receipts_validate_the_inner_admission_and_command() {
        for (data, expected) in [
            (json!({"accepted": true}), Some(Ok(()))),
            (
                json!({"accepted": false}),
                Some(Err("节点明确拒绝了原请求".into())),
            ),
            (json!({}), None),
            (json!({"accepted": "true"}), None),
        ] {
            let mut reply = host(true, data);
            reply["command"] = json!("queue_message");
            assert_eq!(
                interpret_receipt(succeeded(reply), Some("queue_message"), true),
                expected
            );
        }
        assert_eq!(
            interpret_receipt(
                succeeded(host(true, json!({"accepted": true}))),
                Some("queue_message"),
                true
            ),
            None
        );
    }
    #[test]
    fn transport_success_preserves_a_host_refusal() {
        assert_eq!(
            interpret_receipt(succeeded(host(false, Value::Null)), Some("prompt"), false),
            Some(Err("refused".into()))
        );
        assert!(matches!(
            interpret_receipt(
                succeeded(host(true, json!({"accepted":false}))),
                Some("prompt"),
                false
            ),
            Some(Err(_))
        ));
        assert_eq!(
            interpret_receipt(
                succeeded(host(true, json!({"accepted":true}))),
                Some("prompt"),
                false
            ),
            Some(Ok(()))
        );
    }
    #[test]
    fn malformed_and_wrong_command_results_cannot_unlock() {
        for value in [json!({}), json!({"success":true}), host(true, Value::Null)] {
            assert_eq!(
                interpret_receipt(succeeded(value), Some("set_model"), false),
                None
            );
        }
    }
    #[test]
    fn queue_and_dialog_require_real_boolean_acceptance() {
        for value in [json!({}), json!({"accepted":"true"}), json!({"accepted":1})] {
            assert_eq!(interpret_receipt(succeeded(value), None, true), None);
        }
        assert_eq!(
            interpret_receipt(succeeded(json!({"accepted":true})), None, true),
            Some(Ok(()))
        );
        assert!(matches!(
            interpret_receipt(succeeded(json!({"accepted":false})), None, true),
            Some(Err(_))
        ));
    }
}
