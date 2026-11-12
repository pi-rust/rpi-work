use work_client::{SessionPreferences, SessionSummary, WorkspaceClient};

use crate::features::projects::management::{self, SessionAction};

pub enum MutationResult {
    Preferences {
        preferences: SessionPreferences,
        message: String,
    },
    Branch {
        session: SessionSummary,
        message: String,
    },
}

/// Execute session metadata and branch mutations through the typed client.
/// UI state, epoch guards and notifications remain with the workspace page.
pub async fn execute(
    client: &WorkspaceClient,
    action: SessionAction,
    source: &SessionSummary,
) -> Result<MutationResult, String> {
    let id = source.session_id.as_str();
    match action {
        SessionAction::Archive(_, archived) => {
            let preferences = client
                .session_preferences(id, Some(archived), None)
                .await
                .map_err(management::node_error)?;
            Ok(MutationResult::Preferences {
                preferences,
                message: if archived {
                    "会话已归档，历史和草稿保留。"
                } else {
                    "会话已恢复。"
                }
                .to_owned(),
            })
        }
        SessionAction::Pin(_, pinned) => {
            let preferences = client
                .session_preferences(id, None, Some(pinned))
                .await
                .map_err(management::node_error)?;
            Ok(MutationResult::Preferences {
                preferences,
                message: if pinned {
                    "会话已置顶。"
                } else {
                    "已取消置顶。"
                }
                .to_owned(),
            })
        }
        action @ (SessionAction::Fork(_, _) | SessionAction::Clone(_)) => {
            let cloned = matches!(action, SessionAction::Clone(_));
            let fork_entry = match &action {
                SessionAction::Fork(_, entry) => entry.as_deref(),
                _ => None,
            };
            let base = source
                .title
                .as_deref()
                .unwrap_or("会话")
                .chars()
                .take(64)
                .collect::<String>();
            let title = format!("{} · {}", base, if cloned { "副本" } else { "分支" });
            let mutation = if cloned {
                client
                    .clone_session(id, Some(&title))
                    .await
                    .map_err(management::node_error)?
            } else {
                client
                    .fork_session(id, fork_entry, Some(&title))
                    .await
                    .map_err(management::node_error)?
            };
            let mut branch = client
                .get_session(&mutation.session_id)
                .await
                .unwrap_or_else(|_| source.clone());
            branch.session_id = mutation.session_id;
            branch.title = Some(title);
            branch.archived = false;
            branch.pinned = false;
            Ok(MutationResult::Branch {
                session: branch,
                message: "已打开新的会话分支，原会话保持不变。".to_owned(),
            })
        }
        SessionAction::Rename(_)
        | SessionAction::Copy(_)
        | SessionAction::Details(_)
        | SessionAction::Design(_) => Err("该会话操作不属于远程 mutation。".to_owned()),
    }
}
