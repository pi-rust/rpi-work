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
    let value = match operation_result(status)? {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
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

fn operation_result(status: OperationStatus) -> Option<Result<Value, String>> {
    match status {
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value },
        } => Some(Ok(value)),
        OperationStatus::KnownResult {
            outcome: OperationOutcome::Failed { error },
        } if error.code != RealtimeErrorCode::OutcomeUnknown => Some(Err(error.message)),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResumeAdmission {
    Accepted,
    Skipped,
}

/// A known response can admit the run, report that there is nothing to resume,
/// or refuse the command. An incomplete or contradictory response stays unknown.
#[must_use]
pub fn resume_admission(reply: HostResponse<Value>) -> Option<Result<ResumeAdmission, String>> {
    if reply.response_type != "response" || reply.command != "resume_pending" {
        return None;
    }
    if !reply.success {
        return Some(Err(reply
            .error
            .unwrap_or_else(|| "节点拒绝继续队列".into())));
    }
    let data = reply.data?;
    match (data.get("accepted"), data.get("resumed")) {
        (Some(Value::Bool(true)), None) => Some(Ok(ResumeAdmission::Accepted)),
        (None | Some(Value::Bool(false)), Some(Value::Bool(false))) => {
            Some(Ok(ResumeAdmission::Skipped))
        }
        (Some(Value::Bool(false)), None) => Some(Err("节点拒绝继续队列".into())),
        _ => None,
    }
}

#[must_use]
pub fn interpret_resume_receipt(
    status: OperationStatus,
) -> Option<Result<ResumeAdmission, String>> {
    match operation_result(status)? {
        Ok(value) => resume_admission(serde_json::from_value(value).ok()?),
        Err(error) => Some(Err(error)),
    }
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
    fn resume_receipt_keeps_accepted_skipped_rejected_and_unknown_distinct() {
        for (data, expected) in [
            (
                json!({"accepted":true}),
                Some(Ok(ResumeAdmission::Accepted)),
            ),
            (json!({"resumed":false}), Some(Ok(ResumeAdmission::Skipped))),
            (
                json!({"accepted":false}),
                Some(Err("节点拒绝继续队列".into())),
            ),
            (json!({"accepted":true,"resumed":false}), None),
            (json!({"accepted":"true"}), None),
            (json!({}), None),
        ] {
            let mut reply = host(true, data);
            reply["command"] = json!("resume_pending");
            assert_eq!(interpret_resume_receipt(succeeded(reply)), expected);
        }
        assert_eq!(
            interpret_resume_receipt(succeeded(host(true, json!({"accepted":true})))),
            None
        );
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
