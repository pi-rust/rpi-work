use std::collections::{BTreeSet, VecDeque};
use work_core::api::{FileContent, NodeApi, NodeError, NodeErrorKind};

pub(crate) enum Located {
    File(FileContent),
    Directory(String),
    Candidates(Vec<String>),
}

/// Resolve abbreviated links only after an exact read reports a missing file.
/// All reads stay on the selected node and project through the existing port.
pub(crate) async fn read(
    node: &impl NodeApi,
    project: &str,
    path: &str,
    message_link: bool,
) -> Result<Located, NodeError> {
    match node.file(project, path).await {
        Ok(file) => return Ok(Located::File(file)),
        Err(error) if message_link && error.kind == NodeErrorKind::InvalidInput => {
            node.files(project, path).await?;
            return Ok(Located::Directory(path.into()));
        }
        Err(error) if message_link && error.kind == NodeErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut pending = VecDeque::from([String::new()]);
    let mut visited = BTreeSet::new();
    let mut candidates = BTreeSet::new();
    let suffix = format!("/{path}");
    let mut entry_count = 0;
    while let Some(directory) = pending.pop_front() {
        if !visited.insert(directory.clone()) {
            continue;
        }
        if visited.len() > 128 || entry_count > 20_000 {
            return Err(NodeError::new(
                NodeErrorKind::LimitExceeded,
                "项目目录较多，无法确认省略路径的唯一性。请从文件树选择完整路径。",
            ));
        }
        let listing = node.files(project, &directory).await?;
        if listing.path != directory {
            return Err(NodeError::new(
                NodeErrorKind::Internal,
                "节点返回的目录路径不匹配，请刷新项目。",
            ));
        }
        entry_count += listing.entries.len();
        for entry in listing.entries {
            if !super::project_files::valid_child(&directory, &entry) {
                continue;
            }
            if entry.kind == "directory" {
                pending.push_back(entry.path);
            } else if entry.kind == "file" && entry.path.ends_with(&suffix) {
                candidates.insert(entry.path);
            }
        }
    }
    if candidates.len() == 1 {
        return node
            .file(project, candidates.first().expect("one candidate"))
            .await
            .map(Located::File);
    }
    if candidates.is_empty() {
        return Err(NodeError::new(
            NodeErrorKind::NotFound,
            "当前项目中未找到该文件。请确认消息中的路径，或刷新文件树后选择文件。",
        ));
    }
    Ok(Located::Candidates(candidates.into_iter().collect()))
}

pub(crate) fn error_message(error: &NodeError) -> String {
    match error.kind {
        NodeErrorKind::NotFound => {
            "文件已移动、删除，或消息中的路径不完整。请刷新文件树并核对当前项目。".into()
        }
        NodeErrorKind::AccessDenied => {
            "节点拒绝读取此路径，请确认文件位于当前项目内且允许访问。".into()
        }
        NodeErrorKind::UnsupportedContent => "此文件格式无法作为 UTF-8 文本预览。".into(),
        NodeErrorKind::LimitExceeded => "文件或目录超出预览范围，请从文件树选择具体文件。".into(),
        NodeErrorKind::InvalidInput => "请选择项目中的普通文件。目录请在文件树中展开。".into(),
        NodeErrorKind::Unavailable | NodeErrorKind::Timeout => {
            "节点暂时无法读取文件，请检查连接后重试。".into()
        }
        _ => "读取文件失败，请刷新文件树后重试。".into(),
    }
}
