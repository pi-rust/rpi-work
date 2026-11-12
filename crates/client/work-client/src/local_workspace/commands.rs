use super::LocalWorkspace;
use crate::{ClientError, HostResponse, QueueCancellation, QueueMutation, QueueSnapshot};
use serde_json::Value;

impl LocalWorkspace {
    pub async fn set_model(&self, session_id: &str, model_id: &str) -> Result<Value, ClientError> {
        self.set_model_with_provider(session_id, model_id, "").await
    }
    pub async fn set_model_with_provider(
        &self,
        session_id: &str,
        model_id: &str,
        provider: &str,
    ) -> Result<Value, ClientError> {
        self.owner
            .set_model_with_provider(session_id, model_id, provider)
            .await
            .map_err(Into::into)
    }
    pub async fn set_thinking_level(
        &self,
        session_id: &str,
        level: &str,
    ) -> Result<Value, ClientError> {
        self.owner
            .set_thinking_level(session_id, level)
            .await
            .map_err(Into::into)
    }
    pub async fn set_active_tools(
        &self,
        session_id: &str,
        tools: &[String],
    ) -> Result<Value, ClientError> {
        self.owner
            .set_active_tools(session_id, tools)
            .await
            .map_err(Into::into)
    }
    pub async fn resume_pending(&self, session_id: &str) -> Result<Value, ClientError> {
        self.owner
            .resume_pending(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn compact(
        &self,
        session_id: &str,
        custom_instructions: Option<&str>,
    ) -> Result<Value, ClientError> {
        self.owner
            .compact(session_id, custom_instructions)
            .await
            .map_err(Into::into)
    }
    pub async fn queue(
        &self,
        session_id: &str,
        include_completed: bool,
    ) -> Result<QueueSnapshot, ClientError> {
        self.owner
            .queue(session_id, include_completed)
            .await
            .map_err(Into::into)
    }
    pub async fn queue_message(
        &self,
        session_id: &str,
        message: &str,
        deliver_as: &str,
    ) -> Result<QueueMutation, ClientError> {
        self.owner
            .queue_message(session_id, message, deliver_as)
            .await
            .map_err(Into::into)
    }
    pub async fn cancel_queued(
        &self,
        session_id: &str,
        entry_id: &str,
    ) -> Result<QueueCancellation, ClientError> {
        self.owner
            .cancel_queued(session_id, entry_id)
            .await
            .map_err(Into::into)
    }
    pub async fn clear_queue(&self, session_id: &str) -> Result<Value, ClientError> {
        self.owner.clear_queue(session_id).await.map_err(Into::into)
    }
    pub async fn dialog_response(
        &self,
        session_id: &str,
        request_id: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        self.owner
            .dialog_response(session_id, request_id, value)
            .await
            .map_err(Into::into)
    }
    pub async fn cancel_dialog(
        &self,
        session_id: &str,
        request_id: &str,
    ) -> Result<Value, ClientError> {
        self.owner
            .cancel_dialog(session_id, request_id)
            .await
            .map_err(Into::into)
    }
    pub async fn prompt(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<HostResponse<Value>, ClientError> {
        self.owner
            .prompt(session_id, message)
            .await
            .map_err(Into::into)
    }
    pub async fn abort(&self, session_id: &str) -> Result<HostResponse<Value>, ClientError> {
        self.owner.abort(session_id).await.map_err(Into::into)
    }
}
