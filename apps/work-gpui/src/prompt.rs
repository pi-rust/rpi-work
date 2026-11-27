use rpi_extcore::prompt::{PromptBlock, PromptContent};
use serde_json::Value;
use work_core::api::realtime::OperationRef;
use work_core::{ConnectionState, RunPhase};
use work_viewmodel::{diff_view::DraftFeedback, session_controls::PendingRequests};

use crate::shell::{attachments::Attachment, editor::Buffer};

const MAX_FEEDBACK: usize = 32;
const MAX_DRAFT: usize = 32_000;

pub struct Feedback {
    pub id: u64,
    pub diff: Option<DiffAnchor>,
    pub location: String,
    pub comment: String,
    message: String,
}

pub struct DiffAnchor {
    pub path: String,
    pub staged: bool,
    pub index: usize,
    pub revision: String,
}

pub(crate) struct FileReference {
    pub(crate) id: u64,
    pub(crate) path: String,
    message: String,
}

#[derive(Default)]
pub struct Draft {
    pub input: Buffer,
    skill_instruction: Option<String>,
    pub feedback: Vec<Feedback>,
    next_feedback: u64,
    pub(crate) references: Vec<FileReference>,
    next_reference: u64,
    pub attachments: Vec<Attachment>,
    pub attachment_notice: Option<String>,
    next_attachment: u64,
    request: Option<Submission>,
    pub notice: Option<String>,
}

pub(crate) struct SubmittedDraft {
    revision: u64,
    last_feedback: Option<u64>,
    last_reference: Option<u64>,
    last_attachment: Option<u64>,
}

struct Submission {
    stage: Stage,
    draft: Option<SubmittedDraft>,
    saw_activity: bool,
    settled: bool,
}

enum Stage {
    Sending,
    Accepted,
    Unknown(Option<OperationRef>),
}

impl Draft {
    pub(crate) fn prefill_skill(&mut self, instruction: &str) {
        let prefix = self
            .skill_instruction
            .as_deref()
            .filter(|previous| self.input.text.starts_with(previous));
        if prefix != Some(instruction) {
            let previous_len = prefix.unwrap_or_default().encode_utf16().count();
            self.input.replace(Some(0..previous_len), instruction);
            self.skill_instruction = Some(instruction.to_owned());
        }
    }

    pub fn add_attachments(&mut self, mut files: Vec<Attachment>) {
        if self.attachments.len() + files.len() >= rpi_extcore::prompt::MAX_CONTENT_BLOCKS {
            self.attachment_notice = Some("每条消息最多添加 63 个附件。".into());
            return;
        }
        // Ensure the whole selection fits the real ordered content contract before editing the draft.
        let blocks = self
            .attachments
            .iter()
            .chain(&files)
            .map(|file| file.block.clone())
            .collect();
        if let Err(error) = PromptContent::new(blocks) {
            self.attachment_notice = Some(error);
            return;
        }
        for file in &mut files {
            self.next_attachment += 1;
            file.id = self.next_attachment;
        }
        self.attachments.append(&mut files);
        self.attachment_notice = None;
    }

    pub(crate) fn restore_queued(&mut self, data: &Value) -> bool {
        let mut text = Vec::new();
        let mut attachments = Vec::new();
        let mut incomplete = false;
        for message in ["steer", "followUp"]
            .iter()
            .filter_map(|key| data[*key].as_array())
            .flatten()
        {
            if let Some(value) = message["text"].as_str().filter(|value| !value.is_empty()) {
                text.push(value);
            }
            for image in message["images"].as_array().into_iter().flatten() {
                let (Some(data), Some(media_type)) =
                    (image["data"].as_str(), image["mediaType"].as_str())
                else {
                    incomplete = true;
                    continue;
                };
                attachments.push(Attachment {
                    id: 0,
                    name: format!("恢复的图片 {}", attachments.len() + 1),
                    block: PromptBlock::Image {
                        data: data.into(),
                        mime_type: media_type.into(),
                    },
                });
            }
        }
        let previous_attachments = self.attachments.len();
        self.input.append_paragraphs(&text.join("\n\n"));
        if !attachments.is_empty() {
            self.add_attachments(attachments);
            incomplete |= self.attachment_notice.is_some();
        }
        if incomplete {
            self.attachment_notice = Some("部分返还的队列图片超出草稿附件限制或格式无效。".into());
        }
        !incomplete && (!text.is_empty() || self.attachments.len() > previous_attachments)
    }

    pub fn add_feedback(&mut self, item: DraftFeedback<'_>) -> bool {
        self.push_feedback(
            Some(DiffAnchor {
                path: item.path.into(),
                staged: item.staged,
                index: item.index,
                revision: item.revision,
            }),
            item.location,
            item.comment.into(),
            item.message,
        )
    }

    pub fn add_page_feedback(&mut self, location: String, comment: &str) -> bool {
        let comment = comment.trim();
        let message = format!("页面批注 · {location}\n{comment}");
        self.push_feedback(None, location, comment.into(), message)
    }

    fn push_feedback(
        &mut self,
        diff: Option<DiffAnchor>,
        location: String,
        comment: String,
        message: String,
    ) -> bool {
        let separator = usize::from(self.message_parts().next().is_some()) * 2;
        if self.feedback.len() >= MAX_FEEDBACK
            || self.message_size() + separator + message.encode_utf16().count() > MAX_DRAFT
        {
            return false;
        }
        self.next_feedback += 1;
        self.feedback.push(Feedback {
            id: self.next_feedback,
            diff,
            location,
            comment,
            message,
        });
        true
    }

    pub(crate) fn add_file_reference(&mut self, path: &str) -> bool {
        if self.references.iter().any(|item| item.path == path) {
            return true;
        }
        let message = format!(
            "引用项目文件 · {}",
            serde_json::Value::String(path.to_owned())
        );
        if self.references.len() >= MAX_FEEDBACK
            || self.message_size() + 2 + message.encode_utf16().count() > MAX_DRAFT
        {
            self.attachment_notice = Some("项目文件引用或消息长度已达上限。".into());
            return false;
        }
        self.next_reference += 1;
        self.references.push(FileReference {
            id: self.next_reference,
            path: path.to_owned(),
            message,
        });
        self.attachment_notice = None;
        true
    }

    fn message_parts(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.input.text.as_str())
            .chain(self.references.iter().map(|item| item.message.as_str()))
            .chain(self.feedback.iter().map(|item| item.message.as_str()))
            .filter(|text| !text.is_empty())
    }

    fn message_size(&self) -> usize {
        self.message_parts()
            .map(|text| text.encode_utf16().count() + 2)
            .sum::<usize>()
            .saturating_sub(2)
    }

    pub fn message_over_limit(&self) -> bool {
        (!self.feedback.is_empty() || !self.references.is_empty())
            && self.message_size() > MAX_DRAFT
    }

    pub fn can_send(&self) -> bool {
        self.input.is_committed()
            && (self.input.can_send()
                || !self.references.is_empty()
                || !self.feedback.is_empty()
                || !self.attachments.is_empty())
            && !self.message_over_limit()
    }

    pub fn message(&self) -> String {
        self.message_parts().collect::<Vec<_>>().join("\n\n")
    }

    pub fn begin(&mut self, include_draft: bool) {
        self.notice = None;
        if include_draft {
            self.attachment_notice = None;
        }
        self.request = Some(Submission {
            stage: Stage::Sending,
            draft: include_draft.then(|| self.submitted()),
            saw_activity: false,
            settled: false,
        });
    }

    pub(crate) fn submitted(&self) -> SubmittedDraft {
        SubmittedDraft {
            revision: self.input.revision,
            last_feedback: self.feedback.last().map(|item| item.id),
            last_reference: self.references.last().map(|item| item.id),
            last_attachment: self.attachments.last().map(|item| item.id),
        }
    }

    pub(crate) fn clear_submitted(&mut self, draft: SubmittedDraft) {
        if self.input.revision == draft.revision {
            self.input.clear_after_send();
        }
        if let Some(last) = draft.last_feedback {
            self.feedback.retain(|item| item.id > last);
        }
        if let Some(last) = draft.last_reference {
            self.references.retain(|item| item.id > last);
        }
        if let Some(last) = draft.last_attachment {
            self.attachments.retain(|item| item.id > last);
        }
    }

    pub fn accept(&mut self) {
        let Some(request) = &mut self.request else {
            return;
        };
        let submitted = request.draft.take();
        request.stage = Stage::Accepted;
        if request.settled {
            self.request = None;
        }
        if let Some(submitted) = submitted {
            self.clear_submitted(submitted);
        }
        self.notice = None;
    }

    pub fn reject(&mut self, reason: String) {
        self.request = None;
        self.notice = Some(reason);
    }

    pub fn unknown(&mut self, operation: Option<OperationRef>) {
        if let Some(request) = &mut self.request {
            request.stage = Stage::Unknown(operation);
        }
        self.notice = Some("发送结果未知，草稿已保留".into());
    }

    pub fn receipt(&self) -> Option<&OperationRef> {
        match &self.request.as_ref()?.stage {
            Stage::Unknown(operation) => operation.as_ref(),
            _ => None,
        }
    }

    pub fn pending(&self) -> PendingRequests {
        let stage = self.request.as_ref().map(|request| &request.stage);
        PendingRequests {
            submitting: matches!(stage, Some(Stage::Sending)),
            awaiting_reply: matches!(stage, Some(Stage::Accepted)),
            uncertain: matches!(stage, Some(Stage::Unknown(_))),
            ..Default::default()
        }
    }

    pub fn observe(&mut self, event: &str, phase: &RunPhase, connection: &ConnectionState) {
        let Some(request) = &mut self.request else {
            return;
        };
        if *connection != ConnectionState::Online {
            return;
        }
        let terminal = matches!(phase, RunPhase::Settled(_));
        if request.saw_activity
            && terminal
            && matches!(event, "agent_settled" | "connection_online" | "")
        {
            request.settled = true;
            if matches!(request.stage, Stage::Accepted) {
                self.request = None;
            }
        } else if !matches!(phase, RunPhase::Ready | RunPhase::Settled(_))
            && !matches!(event, "" | "access" | "connection_online")
        {
            request.saw_activity = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use work_core::SettlementOutcome;

    const ONLINE: ConnectionState = ConnectionState::Online;
    const COMPLETED: RunPhase = RunPhase::Settled(SettlementOutcome::Completed);

    #[test]
    fn acceptance_requires_new_activity_and_settlement() {
        let mut draft = Draft::default();
        draft.input.replace(None, "hello");
        draft.begin(true);
        draft.accept();
        draft.observe("", &RunPhase::Accepted, &ONLINE);
        draft.observe("connection_online", &COMPLETED, &ONLINE);
        assert!(draft.pending().awaiting_reply);
        draft.observe("agent_start", &RunPhase::Accepted, &ONLINE);
        draft.observe("agent_settled", &COMPLETED, &ONLINE);
        assert!(!draft.pending().awaiting_reply);
        assert!(draft.input.text.is_empty());
    }

    #[test]
    fn skill_prefill_replaces_only_its_unedited_prefix() {
        let mut draft = Draft::default();
        let task = "现有草稿 👋";
        let first = "技能甲 👩‍💻\n\n";
        let second = "技能乙\n\n";
        draft.input.replace(None, task);
        draft.prefill_skill(first);
        draft.prefill_skill(first);
        assert_eq!(draft.message(), format!("{first}{task}"));
        draft.prefill_skill(second);
        assert_eq!(draft.message(), format!("{second}{task}"));
        draft.input.replace(Some(0..1), "自定义");
        draft.prefill_skill(first);
        assert_eq!(draft.message(), format!("{first}自定义能乙\n\n{task}"));
    }

    #[test]
    fn acceptance_removes_only_feedback_in_the_submitted_draft() {
        use work_viewmodel::diff_view::feedback_draft;

        let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-before\n+after\n";
        let feedback =
            |comment| feedback_draft(patch, patch, "file.rs", false, 4, comment).unwrap();
        let mut draft = Draft::default();
        assert!(draft.add_feedback(feedback("first")));
        assert!(draft.can_send());
        draft.begin(false);
        draft.accept();
        assert_eq!(draft.feedback.len(), 1);
        draft.begin(true);
        assert!(draft.add_feedback(feedback("later")));
        draft.accept();
        assert_eq!(draft.feedback.len(), 1);
        assert!(draft.message().ends_with("later"));
    }

    #[test]
    fn acknowledgement_retains_references_added_after_submission() {
        let mut draft = Draft::default();
        draft.input.replace(None, "请检查");
        assert!(draft.add_file_reference("src/my file.rs"));
        assert!(draft.add_file_reference("src/my file.rs"));
        assert_eq!(draft.references.len(), 1);
        assert_eq!(
            draft.message(),
            "请检查\n\n引用项目文件 · \"src/my file.rs\""
        );
        draft.begin(true);
        assert!(draft.add_file_reference("src/second.rs"));
        draft.accept();
        assert_eq!(draft.input.text, "");
        assert_eq!(draft.references.len(), 1);
        assert_eq!(draft.references[0].path, "src/second.rs");
    }

    #[test]
    fn acknowledgement_retains_attachments_added_after_submission() {
        use rpi_extcore::prompt::PromptBlock;
        let item = |name: &str| Attachment {
            id: 0,
            name: name.into(),
            block: PromptBlock::Text { text: name.into() },
        };
        let mut draft = Draft::default();
        draft.add_attachments(vec![item("first")]);
        assert!(draft.can_send());
        draft.begin(true);
        draft.add_attachments(vec![item("later")]);
        draft.unknown(None);
        assert_eq!(draft.attachments.len(), 2);
        draft.accept();
        assert_eq!(draft.attachments.len(), 1);
        assert_eq!(draft.attachments[0].name, "later");
    }

    #[test]
    fn late_acceptance_preserves_edits_and_observed_settlement() {
        let mut draft = Draft::default();
        draft.input.replace(None, "hello");
        draft.begin(true);
        draft.input.replace(None, " again");
        draft.observe("agent_start", &RunPhase::Accepted, &ONLINE);
        draft.observe("connection_online", &COMPLETED, &ONLINE);
        draft.unknown(None);
        assert!(draft.pending().uncertain);
        draft.accept();
        assert_eq!(draft.input.text, "hello again");
        assert!(draft.request.is_none());
    }
}
