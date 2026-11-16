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
        let owned_message_block = message_id(message).is_some_and(|id| {
            block.id() == id
                || block.id().starts_with(&format!("{id}-text-"))
                || block.id().starts_with(&format!("{id}-thinking-"))
        });
        owned_message_block
            || message.get("toolCallId").and_then(Value::as_str) == Some(block.id())
            || conversation::message_has_tool_call(message, block.id())
    })
}

#[cfg(test)]
mod tests {
    use super::history_has_block;
    use serde_json::json;
    use work_core::MessageBlock;

    #[test]
    fn canonical_assistant_message_owns_thinking_projection_blocks() {
        let message = json!({"entryId":"answer-1","role":"assistant","content":"answer"});
        let block = MessageBlock::Thinking {
            id: "answer-1-thinking-0".into(),
            text: "private".into(),
        };
        assert!(history_has_block(std::slice::from_ref(&message), &block));
    }
}
