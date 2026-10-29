//! ACP permission conversion. Never infer an option ID or an allow decision.
use serde_json::{Value, json};

use crate::{ClientError, PendingDialog};
#[cfg(any(not(target_arch = "wasm32"), test))]
use crate::{WireEvent, dialog_from_wire};

fn invalid_permission() -> ClientError {
    ClientError::Acp("invalid or ambiguous ACP permission selection".into())
}

fn options(dialog: &PendingDialog) -> Result<&[Value], ClientError> {
    dialog
        .ui
        .raw
        .pointer("/acpPermission/options")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(invalid_permission)
}

fn selected_result(option: &Value) -> Result<Value, ClientError> {
    let id = option
        .get("optionId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(invalid_permission)?;
    if !matches!(
        option.get("kind").and_then(Value::as_str),
        Some("allow_once" | "allow_always" | "reject_once" | "reject_always")
    ) {
        return Err(invalid_permission());
    }
    Ok(json!({"outcome":{"outcome":"selected","optionId":id}}))
}

impl PendingDialog {
    /// Convert the single selected UI label back to the exact negotiated ACP ID.
    /// Invalid, unknown, multi-select or ambiguous answers are errors, not allow.
    pub fn acp_permission_result(&self, answer: Option<&Value>) -> Result<Value, ClientError> {
        if self.response_id.is_none() {
            return Err(invalid_permission());
        }
        let Some(answer) = answer else {
            return Ok(json!({"outcome":{"outcome":"cancelled"}}));
        };
        if answer.get("kind").and_then(Value::as_str) != Some("selector") {
            return Err(invalid_permission());
        }
        let values = answer
            .get("values")
            .and_then(Value::as_array)
            .filter(|values| values.len() == 1)
            .ok_or_else(invalid_permission)?;
        let label = values[0].as_str().ok_or_else(invalid_permission)?;
        let mut matching = options(self)?
            .iter()
            .filter(|option| option.get("name").and_then(Value::as_str) == Some(label));
        let option = matching.next().ok_or_else(invalid_permission)?;
        if matching.next().is_some() {
            return Err(invalid_permission());
        }
        let result = selected_result(option)?;
        self.validate_acp_permission_result(&result)?;
        Ok(result)
    }

    /// Non-interactive opt-in chooses an actual allow option; absent one, cancel.
    pub fn acp_permission_policy(&self, allow: bool) -> Result<Value, ClientError> {
        if allow {
            for option in options(self)? {
                if matches!(
                    option.get("kind").and_then(Value::as_str),
                    Some("allow_once" | "allow_always")
                ) {
                    let result = selected_result(option)?;
                    self.validate_acp_permission_result(&result)?;
                    return Ok(result);
                }
            }
        }
        self.acp_permission_result(None)
    }

    pub(crate) fn validate_acp_permission_result(&self, result: &Value) -> Result<(), ClientError> {
        let outcome = result.get("outcome").ok_or_else(invalid_permission)?;
        match outcome.get("outcome").and_then(Value::as_str) {
            Some("cancelled") if outcome.get("optionId").is_none() => Ok(()),
            Some("selected") => {
                let id = outcome
                    .get("optionId")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid_permission)?;
                let mut matching = options(self)?
                    .iter()
                    .filter(|option| option.get("optionId").and_then(Value::as_str) == Some(id));
                let option = matching.next().ok_or_else(invalid_permission)?;
                if matching.next().is_some() {
                    return Err(invalid_permission());
                }
                selected_result(option).map(|_| ())
            }
            _ => Err(invalid_permission()),
        }
    }
}

/// Preserve original protocol metadata, while adapting presentation to `AskCard`.
#[cfg(any(not(target_arch = "wasm32"), test))]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn permission_event(request_id: Value, params: Value) -> Result<WireEvent, ClientError> {
    if !request_id.is_string() && !request_id.is_number() {
        return Err(invalid_permission());
    }
    let key = request_id
        .as_str()
        .map_or_else(|| request_id.to_string(), str::to_owned);
    let presentation = params
        .get("options")
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .map(|option| {
                    json!({
                        "title": option.get("name"),
                        "permission": option.get("kind"),
                        "optionId": option.get("optionId")
                    })
                })
                .collect::<Vec<_>>()
        });
    let event = crate::wire_event(
        Some(key.clone()),
        None,
        "ui_dialog_request",
        json!({
            "type":"ui_dialog_request", "requestId":key,
            "jsonRpcRequestId":request_id, "sessionId":params.get("sessionId"),
            "toolCallId": params.pointer("/toolCall/toolCallId"),
            "ui": {
                "purpose":"permission", "kind":"selector",
                "question":params.pointer("/toolCall/title"),
                "options":presentation, "acpPermission":params
            }
        }),
    );
    // Reject non-object UI shapes here, not in a later stream consumer.
    dialog_from_wire(&event)?;
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog() -> PendingDialog {
        dialog_from_wire(
            &permission_event(
                json!("4"),
                json!({
                    "sessionId":"s", "toolCall":{"toolCallId":"t","title":"Run tool?"},
                    "options":[
                        {"optionId":"deny-7","name":"No","kind":"reject_once"},
                        {"optionId":"permit-9","name":"Yes","kind":"allow_once"}
                    ]
                }),
            )
            .unwrap(),
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn selects_exact_id_or_cancels_without_inference() {
        let dialog = dialog();
        assert_eq!(dialog.response_id, Some(json!("4")));
        assert_eq!(dialog.ui.purpose.as_deref(), Some("permission"));
        assert_eq!(dialog.tool_call_id.as_deref(), Some("t"));
        for (label, id) in [("No", "deny-7"), ("Yes", "permit-9")] {
            assert_eq!(
                dialog
                    .acp_permission_result(Some(&json!({"kind":"selector","values":[label]})))
                    .unwrap(),
                json!({"outcome":{"outcome":"selected","optionId":id}})
            );
        }
        assert_eq!(
            dialog.acp_permission_policy(true).unwrap(),
            json!({"outcome":{"outcome":"selected","optionId":"permit-9"}})
        );
        assert_eq!(
            dialog.acp_permission_policy(false).unwrap(),
            json!({"outcome":{"outcome":"cancelled"}})
        );
        for answer in [
            json!({}),
            json!({"kind":"input","values":["Yes"]}),
            json!({"kind":"selector","values":[]}),
            json!({"kind":"selector","values":["Yes","No"]}),
            json!({"kind":"selector","values":["allow"]}),
        ] {
            assert!(dialog.acp_permission_result(Some(&answer)).is_err());
        }
    }

    #[test]
    fn rejects_duplicate_names_ids_unknown_kinds_and_missing_metadata() {
        let mut dialog = dialog();
        dialog.ui.raw["acpPermission"]["options"][1]["name"] = json!("No");
        assert!(
            dialog
                .acp_permission_result(Some(&json!({"kind":"selector","values":["No"]})))
                .is_err()
        );
        dialog.ui.raw["acpPermission"]["options"][1]["optionId"] = json!("deny-7");
        assert!(dialog.acp_permission_policy(true).is_err());
        dialog.ui.raw["acpPermission"]["options"] =
            json!([{"optionId":"x","name":"Yes","kind":"unknown"}]);
        assert!(
            dialog
                .acp_permission_result(Some(&json!({"kind":"selector","values":["Yes"]})))
                .is_err()
        );
        assert_eq!(
            dialog.acp_permission_policy(true).unwrap(),
            json!({"outcome":{"outcome":"cancelled"}})
        );
        dialog.response_id = None;
        assert!(dialog.acp_permission_result(None).is_err());
    }
}
