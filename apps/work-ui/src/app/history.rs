use crate::features::conversation::timeline as conversation;
use serde_json::Value;
use work_core::MessageBlock;
pub(crate) fn message_id(message: &Value) -> Option<&str> {
    ["entryId", "messageId", "id"]
        .iter()
        .find_map(|key| message.get(key)?.as_str())
}

pub(crate) fn history_id(message: &Value, index: usize) -> String {
    message_id(message).map_or_else(|| format!("snapshot-{index}"), str::to_owned)
}

pub(crate) fn history_has_block(messages: &[Value], block: &MessageBlock) -> bool {
    messages.iter().any(|message| {
        if let Some(id) = message_id(message) {
            if block.id() == id || block.id().starts_with(&format!("{id}-text-")) {
                return true;
            }
        }
        message.get("toolCallId").and_then(Value::as_str) == Some(block.id())
            || conversation::message_has_tool_call(message, block.id())
    })
}
