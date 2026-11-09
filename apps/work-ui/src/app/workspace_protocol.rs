use serde_json::Value;
use work_client::{ClientError, HostResponse, SessionSummary, WorkspaceClient};
use work_core::api::realtime::{OperationOutcome, OperationStatus};

pub(crate) async fn reconcile_operation_value(
    client: &WorkspaceClient,
    operation: work_core::api::realtime::OperationRef,
) -> Result<Value, String> {
    let receipt = format!("{}/{}", operation.owner_epoch, operation.operation_id);
    match client.operation_status(&operation).await {
        Ok(OperationStatus::KnownResult {
            outcome: OperationOutcome::Succeeded { value },
        }) => Ok(value),
        Ok(OperationStatus::KnownResult {
            outcome: OperationOutcome::Failed { .. },
        }) => Err(format!(
            "节点已记录失败（receipt {receipt}）；本地附件仍保留，请修正后再发送。"
        )),
        Ok(
            OperationStatus::OutcomeUnknown
            | OperationStatus::Reserved
            | OperationStatus::Dispatching,
        ) => Err(format!(
            "执行结果尚未确认（receipt {receipt}）；未重发，本地附件仍保留，请重连后查询。"
        )),
        Err(error) => Err(format!(
            "执行结果未确认（receipt {receipt}），查询失败：{error}；未重发，本地附件仍保留。"
        )),
    }
}

pub(crate) fn session_failure_message(session: &SessionSummary) -> Option<String> {
    let message = session
        .error
        .as_deref()
        .or_else(|| {
            session
                .latest_settlement
                .as_ref()
                .and_then(|settlement| {
                    settlement
                        .get("error")
                        .or_else(|| settlement.get("message"))
                })
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|message| !message.is_empty())?;
    Some(message.chars().take(2000).collect())
}

pub(crate) async fn reconcile_prompt_response(
    client: &WorkspaceClient,
    response: Result<HostResponse<Value>, ClientError>,
) -> Result<HostResponse<Value>, String> {
    match response {
        Ok(response) => Ok(response),
        Err(ClientError::OperationOutcomeUnknown { operation }) => {
            let receipt = format!("{}/{}", operation.owner_epoch, operation.operation_id);
            reconcile_operation_value(client, operation).await.and_then(|value| {
                serde_json::from_value(value).map_err(|_| {
                    format!(
                        "执行结果已记录但响应格式异常（receipt {receipt}）；附件未清理，请核对历史。"
                    )
                })
            })
        }
        Err(error) => Err(error.to_string()),
    }
}
