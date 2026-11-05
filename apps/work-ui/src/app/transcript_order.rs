//! Stable display slots for canonical history and the append-only run projection.
//!
//! Only identities are retained: callers always render the current input at `source`.
//! Projection indices must stay stable within a scope; reset the scope when a run
//! replaces its projection rather than appending to it.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use work_core::MessageBlock;

use super::history::{history_has_block, history_id};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RowSource {
    History(usize),
    Projection(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OrderedRow {
    pub key: u64,
    pub source: RowSource,
}

#[derive(Default)]
pub(crate) struct TranscriptOrder {
    scope: Option<String>,
    next_key: u64,
    slots: Vec<Slot>,
}

struct Slot {
    key: u64,
    history_id: Option<String>,
    projections: Vec<usize>,
    optimistic: bool,
}

struct Candidate {
    source: RowSource,
    history_id: Option<String>,
    projections: Vec<usize>,
}

impl TranscriptOrder {
    /// An optimistic user row receives the host's real ID when its echo arrives.
    /// Only the active scope may transfer that existing display slot.
    pub fn rekey_history(&mut self, scope: &str, old: &str, new: &str) {
        if self.scope.as_deref() == Some(scope) {
            for slot in &mut self.slots {
                if slot.history_id.as_deref() == Some(old) {
                    slot.history_id = Some(new.to_owned());
                }
            }
        }
    }

    pub fn update(
        &mut self,
        scope: &str,
        history: &[Value],
        blocks: &[MessageBlock],
    ) -> Vec<OrderedRow> {
        if self.scope.as_deref() != Some(scope) {
            self.scope = Some(scope.to_owned());
            self.slots.clear();
            // Never recycle a DOM key, including across scope changes.
        }

        let (candidates, projection_candidates) = build_candidates(history, blocks);

        let mut history_candidates = HashMap::new();
        for (index, candidate) in candidates.iter().enumerate() {
            if let Some(id) = candidate.history_id.as_deref() {
                history_candidates.entry(id).or_insert(index);
            }
        }

        let optimistic: HashSet<_> = self
            .slots
            .iter()
            .filter(|slot| slot.optimistic)
            .map(|slot| slot.key)
            .collect();
        let mut selected = vec![false; candidates.len()];
        let mut ordered = Vec::with_capacity(candidates.len());
        // Walk display slots, not canonical order: the earliest displayed slot
        // wins when several projection blocks become one canonical message.
        for slot in &self.slots {
            let candidate_index = slot
                .history_id
                .as_deref()
                .and_then(|id| history_candidates.get(id).copied())
                .or_else(|| {
                    slot.projections
                        .iter()
                        .find_map(|&index| projection_candidates.get(index).copied())
                });
            if let Some(index) = candidate_index {
                if !selected[index] {
                    selected[index] = true;
                    ordered.push((index, slot.key));
                }
            }
        }

        // New history belongs before its next surviving canonical history row
        // (pagination / a late user message). Without that anchor, append it:
        // a just-completed message must not jump ahead of an older projection.
        for index in 0..history.len() {
            if selected[index] {
                continue;
            }
            let successor = (index + 1..history.len()).find(|&next| {
                selected[next]
                    && history[next]["__rpiPendingUser"] != true
                    && !self.slots.iter().any(|slot| {
                        slot.optimistic && slot.history_id == candidates[next].history_id
                    })
            });
            let position = successor
                .and_then(|next| ordered.iter().position(|&(candidate, _)| candidate == next))
                .unwrap_or(ordered.len());
            ordered.insert(position, (index, self.allocate_key()));
            selected[index] = true;
        }
        // Independent projection candidates were built in projection-index order.
        for (index, is_selected) in selected.iter().enumerate().skip(history.len()) {
            if !is_selected {
                ordered.push((index, self.allocate_key()));
            }
        }

        reconcile_canonical_order(&mut ordered, history, &optimistic);

        let rows = ordered
            .iter()
            .map(|&(index, key)| OrderedRow {
                key,
                source: candidates[index].source,
            })
            .collect();
        self.slots = ordered
            .into_iter()
            .map(|(index, key)| Slot {
                key,
                history_id: candidates[index].history_id.clone(),
                projections: candidates[index].projections.clone(),
                optimistic: optimistic.contains(&key)
                    || (index < history.len() && history[index]["__rpiPendingUser"] == true),
            })
            .collect();
        rows
    }

    fn allocate_key(&mut self) -> u64 {
        let key = self.next_key;
        self.next_key = self
            .next_key
            .checked_add(1)
            .expect("transcript key exhausted");
        key
    }
}

fn build_candidates(history: &[Value], blocks: &[MessageBlock]) -> (Vec<Candidate>, Vec<usize>) {
    let mut candidates: Vec<_> = history
        .iter()
        .enumerate()
        .map(|(index, message)| Candidate {
            source: RowSource::History(index),
            history_id: Some(history_id(message, index)),
            projections: Vec::new(),
        })
        .collect();
    let mut projection_candidates = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        // Calls and results can share a tool ID. Assign a block only to the
        // first owner, never use that ID to merge two distinct historical rows.
        let owner = history
            .iter()
            .position(|message| history_has_block(std::slice::from_ref(message), block));
        let candidate_index = if let Some(owner) = owner {
            candidates[owner].projections.push(index);
            owner
        } else {
            let candidate_index = candidates.len();
            candidates.push(Candidate {
                source: RowSource::Projection(index),
                history_id: None,
                projections: vec![index],
            });
            candidate_index
        };
        projection_candidates.push(candidate_index);
    }
    (candidates, projection_candidates)
}

// Authoritative history can repair a remote user's late echo, even when both
// rows already have slots. Unresolved projection and optimistic rows stay put.
fn reconcile_canonical_order(
    ordered: &mut [(usize, u64)],
    history: &[Value],
    optimistic: &HashSet<u64>,
) {
    let positions: Vec<_> = ordered
        .iter()
        .enumerate()
        .filter(|(_, (index, key))| {
            *index < history.len()
                && history[*index]["__rpiPendingUser"] != true
                && !optimistic.contains(key)
        })
        .map(|(position, _)| position)
        .collect();
    let mut canonical: Vec<_> = positions
        .iter()
        .map(|&position| ordered[position])
        .collect();
    canonical.sort_by_key(|(index, _)| *index);
    for (position, row) in positions.into_iter().zip(canonical) {
        ordered[position] = row;
    }
}

#[cfg(test)]
mod tests {
    use super::{OrderedRow, RowSource, TranscriptOrder};
    use serde_json::{Value, json};
    use work_core::{MessageBlock, ToolStatus};

    fn message(id: &str) -> Value {
        json!({"id": id, "role": "assistant", "content": "same text"})
    }

    fn block(id: &str) -> MessageBlock {
        MessageBlock::Text {
            id: id.to_owned(),
            text: "same text".to_owned(),
        }
    }

    fn sources(rows: &[OrderedRow]) -> Vec<RowSource> {
        rows.iter().map(|row| row.source).collect()
    }

    #[test]
    fn initializes_in_history_then_uncovered_projection_order() {
        let mut order = TranscriptOrder::default();
        let rows = order.update(
            "run",
            &[message("a"), message("b")],
            &[block("a-text-0"), block("live-1"), block("live-2")],
        );
        assert_eq!(
            sources(&rows),
            vec![
                RowSource::History(0),
                RowSource::History(1),
                RowSource::Projection(1),
                RowSource::Projection(2),
            ]
        );
    }

    #[test]
    fn new_complete_history_does_not_overtake_old_projection() {
        let mut order = TranscriptOrder::default();
        let blocks = [block("live")];
        let before = order.update("run", &[message("old")], &blocks);
        let after = order.update("run", &[message("old"), message("new")], &blocks);
        assert_eq!(
            sources(&after),
            vec![
                RowSource::History(0),
                RowSource::Projection(0),
                RowSource::History(1),
            ]
        );
        assert_eq!(&after[..2], &before);
        assert!(after[2].key > before[1].key);
    }

    #[test]
    fn projection_upgrades_to_history_in_the_same_slot() {
        let mut order = TranscriptOrder::default();
        let blocks = [block("answer-text-0")];
        let before = order.update("run", &[], &blocks);
        let after = order.update("run", &[message("answer")], &blocks);
        assert_eq!(
            after,
            vec![OrderedRow {
                key: before[0].key,
                source: RowSource::History(0),
            }]
        );
        // The history ID keeps the slot even when the projection disappears.
        assert_eq!(order.update("run", &[message("answer")], &[]), after);
    }

    #[test]
    fn multiple_blocks_collapse_to_the_earliest_display_slot() {
        let mut order = TranscriptOrder::default();
        let blocks = [
            block("answer-text-0"),
            block("answer-text-1"),
            block("tail"),
        ];
        let before = order.update("run", &[], &blocks);
        let after = order.update("run", &[message("answer")], &blocks);
        assert_eq!(
            after,
            vec![
                OrderedRow {
                    key: before[0].key,
                    source: RowSource::History(0),
                },
                before[2].clone(),
            ]
        );
        assert_eq!(order.update("run", &[message("answer")], &blocks), after);
    }

    #[test]
    fn collapse_uses_display_order_not_projection_index_order() {
        let mut order = TranscriptOrder::default();
        // History puts projection index 1 before the independent index 0.
        let before = order.update(
            "run",
            &[message("temporary")],
            &[block("loose"), block("temporary-text-0")],
        );
        assert_eq!(
            sources(&before),
            vec![RowSource::History(0), RowSource::Projection(0)]
        );
        let after = order.update(
            "run",
            &[message("canonical")],
            &[block("canonical-text-0"), block("canonical-text-1")],
        );
        assert_eq!(
            after,
            vec![OrderedRow {
                key: before[0].key,
                source: RowSource::History(0),
            }]
        );
    }

    #[test]
    fn history_id_rename_uses_stable_projection_index() {
        let mut order = TranscriptOrder::default();
        let before = order.update("run", &[], &[block("temporary")]);
        let renamed = [block("canonical-text-0")];
        assert_eq!(order.update("run", &[], &renamed), before);
        let canonical = order.update("run", &[message("canonical")], &renamed);
        assert_eq!(canonical[0].key, before[0].key);
        assert_eq!(canonical[0].source, RowSource::History(0));
        let renamed_again = order.update("run", &[message("final")], &[block("final-text-0")]);
        assert_eq!(renamed_again, canonical);
    }

    #[test]
    fn tool_call_and_result_remain_separate_history_rows() {
        let mut order = TranscriptOrder::default();
        let blocks = [MessageBlock::ToolCall {
            id: "call-1".to_owned(),
            name: "read".to_owned(),
            arguments: "{}".to_owned(),
            output: String::new(),
            status: ToolStatus::Running,
        }];
        let before = order.update("run", &[], &blocks);
        let call = json!({
            "id": "assistant-1", "role": "assistant",
            "content": [{"type": "toolCall", "id": "call-1", "name": "read"}]
        });
        let result = json!({
            "id": "result-1", "role": "toolResult", "toolCallId": "call-1",
            "content": "same text"
        });
        let history = [call, result];
        let after = order.update("run", &history, &blocks);
        assert_eq!(
            sources(&after),
            vec![RowSource::History(0), RowSource::History(1)]
        );
        assert_eq!(after[0].key, before[0].key);
        assert_ne!(after[0].key, after[1].key);
        assert_eq!(order.update("run", &history, &blocks), after);

        // Ownership follows history order, not a preference for the call role.
        let reversed = [history[1].clone(), history[0].clone()];
        let mut fresh = TranscriptOrder::default();
        let projected = fresh.update("run", &[], &blocks);
        let rows = fresh.update("run", &reversed, &blocks);
        assert_eq!(
            sources(&rows),
            vec![RowSource::History(0), RowSource::History(1)]
        );
        assert_eq!(rows[0].key, projected[0].key);
    }

    #[test]
    fn pagination_inserts_before_the_next_history_anchor() {
        let mut order = TranscriptOrder::default();
        let before = order.update("run", &[message("c"), message("d")], &[]);
        let after = order.update(
            "run",
            &[message("a"), message("b"), message("c"), message("d")],
            &[],
        );
        assert_eq!(
            sources(&after),
            (0..4).map(RowSource::History).collect::<Vec<_>>()
        );
        assert_eq!(after[2].key, before[0].key);
        assert_eq!(after[3].key, before[1].key);
    }

    #[test]
    fn late_user_is_inserted_before_an_upgraded_assistant_anchor() {
        let mut order = TranscriptOrder::default();
        let blocks = [block("assistant-text-0")];
        let before = order.update("run", &[message("old")], &blocks);
        let after = order.update(
            "run",
            &[
                message("old"),
                json!({"id": "user", "role": "user", "content": "question"}),
                message("assistant"),
            ],
            &blocks,
        );
        assert_eq!(
            sources(&after),
            vec![
                RowSource::History(0),
                RowSource::History(1),
                RowSource::History(2)
            ]
        );
        assert_eq!(after[0].key, before[0].key);
        assert_eq!(after[2].key, before[1].key);
    }

    #[test]
    fn scope_reset_discards_anchors_but_never_reuses_keys() {
        let mut order = TranscriptOrder::default();
        let blocks = [block("live")];
        let before = order.update("one", &[], &blocks);
        let after = order.update("two", &[message("new")], &blocks);
        assert_eq!(
            sources(&after),
            vec![RowSource::History(0), RowSource::Projection(0)]
        );
        assert!(after[0].key > before[0].key);
        assert!(after[1].key > after[0].key);
        let returned = order.update("one", &[], &blocks);
        assert!(returned[0].key > after[1].key);
    }

    #[test]
    fn identical_text_with_distinct_ids_is_not_deduplicated() {
        let mut order = TranscriptOrder::default();
        let history = [message("a"), message("b")];
        let blocks = [block("c"), block("d")];
        let before = order.update("run", &history, &blocks);
        assert_eq!(before.len(), 4);
        let keys: std::collections::HashSet<_> = before.iter().map(|row| row.key).collect();
        assert_eq!(keys.len(), 4);
        assert_eq!(order.update("run", &history, &blocks), before);
    }

    #[test]
    fn missing_history_id_uses_the_history_helper_fallback() {
        let mut order = TranscriptOrder::default();
        let before = order.update("run", &[json!({"content": "first"})], &[]);
        let after = order.update("run", &[json!({"content": "changed"})], &[]);
        assert_eq!(after, before);
    }

    #[test]
    fn late_user_then_canonical_assistant_repairs_known_history_precedence() {
        let mut order = TranscriptOrder::default();
        let blocks = [block("assistant-text-0")];
        let first = order.update("run", &[], &blocks);
        let late = order.update(
            "run",
            &[json!({"id":"user","role":"user","content":"question"})],
            &blocks,
        );
        assert_eq!(late[0].key, first[0].key);
        let canonical = order.update(
            "run",
            &[
                json!({"id":"user","role":"user","content":"question"}),
                message("assistant"),
            ],
            &blocks,
        );
        assert_eq!(
            sources(&canonical),
            vec![RowSource::History(0), RowSource::History(1)]
        );
        assert_eq!(canonical[0].key, late[1].key);
        assert_eq!(canonical[1].key, first[0].key);
    }

    #[test]
    fn a_new_full_response_cannot_insert_before_the_optimistic_question() {
        let mut order = TranscriptOrder::default();
        let pending =
            json!({"id":"pending", "role":"user", "content":"question", "__rpiPendingUser":true});
        let before = order.update("run", std::slice::from_ref(&pending), &[]);
        let after = order.update(
            "run",
            &[message("reply"), pending],
            &[block("reply-text-0")],
        );
        assert_eq!(
            sources(&after),
            vec![RowSource::History(1), RowSource::History(0)]
        );
        assert_eq!(after[0].key, before[0].key);
    }

    #[test]
    fn optimistic_user_echo_keeps_its_slot_before_an_early_reply() {
        let mut order = TranscriptOrder::default();
        let pending =
            json!({"id":"pending", "role":"user", "content":"question", "__rpiPendingUser":true});
        let before = order.update("run", &[pending], &[block("early-text-0")]);
        order.rekey_history("other", "pending", "wrong-scope");
        order.rekey_history("run", "pending", "persisted-user");
        // The early assistant body was merged before the delayed user echo.
        // That live append order must not overrule our observed request order.
        let after = order.update(
            "run",
            &[message("early"), message("persisted-user")],
            &[block("early-text-0")],
        );
        assert_eq!(
            sources(&after),
            vec![RowSource::History(1), RowSource::History(0)]
        );
        assert_eq!(after[0].key, before[0].key);
        assert_eq!(after[1].key, before[1].key);
    }

    #[test]
    fn vanished_rows_are_removed_and_not_resurrected() {
        let mut order = TranscriptOrder::default();
        let before = order.update("run", &[message("a"), message("b")], &[]);
        let after = order.update("run", &[message("b")], &[]);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].key, before[1].key);
        assert_eq!(after[0].source, RowSource::History(0));
        assert_eq!(order.update("run", &[], &[]), Vec::<OrderedRow>::new());
        let returned = order.update("run", &[message("a")], &[]);
        assert!(returned[0].key > before[1].key);
    }
}
