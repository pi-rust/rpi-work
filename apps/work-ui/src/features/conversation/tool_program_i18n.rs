#![allow(dead_code)]

//! Minimal localized copy for the Tool Program workbench.
//!
//! The application does not yet expose a global locale setting.  Keeping the
//! copy behind a locale-aware facade lets this feature migrate its strings now
//! without inventing a second settings system; `zh_cn()` remains the current
//! application default and `en_us()` is useful for future locale plumbing.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Locale {
    ZhCn,
    EnUs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Key {
    Workbench,
    ToolProgram,
    ReadingToolProgram,
    ReadFailed,
    LastSynced,
    CollapseAll,
    ToolName,
    ToolNamePlaceholder,
    ChildFilter,
    PhaseFilter,
    All,
    Running,
    Failed,
    Completed,
    Attention,
    ContainsBody,
    Loading,
    LoadMoreChildren,
    ChildrenPageFailed,
    ResyncRequired,
    Resyncing,
    Resync,
    Transport,
    RetryCount,
    StopReconnect,
    Reconnect,
    RetryRealtime,
    Connected,
    Connecting,
    Child,
    State,
    Outcome,
    TraceUnavailable,
    Parent,
    Collapse,
    ViewPhases,
    LoadingPhases,
    Error,
    Cancelled,
    Interrupted,
    LoadMore,
    ViewBody,
    Viewer,
    LoadedBytes,
    DownloadLoadedBody,
    Copied,
    CopyFailed,
    CopyCompleteText,
    BodyIncomplete,
    BodyLengthMismatch,
    BodyCompleteHash,
    BodyHashMismatch,
    ViewChunks,
    LoadMorePhases,
    BinaryChunk,
    UndecodableChunk,
    ChunkVerified,
    ChunkInvalid,
    PageHidden,
    AutoReconnectPaused,
    SocketInterrupted,
    SocketDisconnected,
    SocketUnavailable,
    RetryLimit,
    RetryWindowLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Copy {
    locale: Locale,
}

impl Copy {
    pub(crate) const fn for_locale(locale: Locale) -> Self {
        Self { locale }
    }

    pub(crate) const fn zh_cn() -> Self {
        Self::for_locale(Locale::ZhCn)
    }

    pub(crate) const fn en_us() -> Self {
        Self::for_locale(Locale::EnUs)
    }

    pub(crate) const fn text(self, key: Key) -> &'static str {
        match self.locale {
            Locale::ZhCn => zh_cn(key),
            Locale::EnUs => en_us(key),
        }
    }
}

const fn zh_cn(key: Key) -> &'static str {
    match key {
        Key::Workbench => "Tool Program 工作台",
        Key::ToolProgram => "Tool Program",
        Key::ReadingToolProgram => "正在读取 Tool Program…",
        Key::ReadFailed => "Tool Program 读取失败",
        Key::LastSynced => "最后同步",
        Key::CollapseAll => "全部收起",
        Key::ToolName => "工具名",
        Key::ToolNamePlaceholder => "筛选 tool",
        Key::ChildFilter => "Child 筛选",
        Key::PhaseFilter => "Phase 筛选",
        Key::All => "全部",
        Key::Running => "运行中",
        Key::Failed => "失败",
        Key::Completed => "已完成",
        Key::Attention => "异常/中断",
        Key::ContainsBody => "包含 body",
        Key::Loading => "读取中…",
        Key::LoadMoreChildren => "加载更多子任务",
        Key::ChildrenPageFailed => "子任务分页失败",
        Key::ResyncRequired => "需要重新同步",
        Key::Resyncing => "同步中…",
        Key::Resync => "重新同步",
        Key::Transport => "通道：WorkspaceClient realtime subscribe",
        Key::RetryCount => "次重连",
        Key::StopReconnect => "停止重连",
        Key::Reconnect => "重新连接",
        Key::RetryRealtime => "重试实时连接",
        Key::Connected => "实时已连接",
        Key::Connecting => "实时连接中…",
        Key::Child => "子任务",
        Key::State => "状态",
        Key::Outcome => "结果",
        Key::TraceUnavailable => "Trace：未提供关联",
        Key::Parent => "父任务",
        Key::Collapse => "收起",
        Key::ViewPhases => "查看 phases",
        Key::LoadingPhases => "正在读取 phases…",
        Key::Error => "错误",
        Key::Cancelled => "已取消",
        Key::Interrupted => "中断",
        Key::LoadMore => "加载更多",
        Key::ViewBody => "查看 body",
        Key::Viewer => "查看器",
        Key::LoadedBytes => "已加载",
        Key::DownloadLoadedBody => "下载已加载 body",
        Key::Copied => "已复制",
        Key::CopyFailed => "复制失败",
        Key::CopyCompleteText => "复制完整文本",
        Key::BodyIncomplete => "内容未完整加载",
        Key::BodyLengthMismatch => "返回内容长度与 descriptor 不一致",
        Key::BodyCompleteHash => "内容完整 · SHA-256 通过",
        Key::BodyHashMismatch => "整体 SHA-256 不匹配",
        Key::ViewChunks => "查看 chunks",
        Key::LoadMorePhases => "加载更多 phases",
        Key::BinaryChunk => "二进制 chunk",
        Key::UndecodableChunk => "无法解码 chunk",
        Key::ChunkVerified => "校验通过",
        Key::ChunkInvalid => "校验失败",
        Key::PageHidden => "页面不可见，自动重连已暂停",
        Key::AutoReconnectPaused => "自动重连已暂停",
        Key::SocketInterrupted => "实时 WebSocket 连接中断，准备重连",
        Key::SocketDisconnected => "实时 WebSocket 已断开，准备重连",
        Key::SocketUnavailable => "实时 WebSocket 未连接，准备重连",
        Key::RetryLimit => "自动重连次数已达上限",
        Key::RetryWindowLimit => "自动重连总时长已达上限",
    }
}

const fn en_us(key: Key) -> &'static str {
    match key {
        Key::Workbench => "Tool Program Workbench",
        Key::ToolProgram => "Tool Program",
        Key::ReadingToolProgram => "Reading Tool Program…",
        Key::ReadFailed => "Tool Program read failed",
        Key::LastSynced => "Last synced",
        Key::CollapseAll => "Collapse all",
        Key::ToolName => "Tool name",
        Key::ToolNamePlaceholder => "Filter tool",
        Key::ChildFilter => "Child filter",
        Key::PhaseFilter => "Phase filter",
        Key::All => "All",
        Key::Running => "Running",
        Key::Failed => "Failed",
        Key::Completed => "Completed",
        Key::Attention => "Error/interrupted",
        Key::ContainsBody => "Has body",
        Key::Loading => "Loading…",
        Key::LoadMoreChildren => "Load more children",
        Key::ChildrenPageFailed => "Child page failed",
        Key::ResyncRequired => "Resync required",
        Key::Resyncing => "Syncing…",
        Key::Resync => "Resync",
        Key::Transport => "Transport: WorkspaceClient realtime subscribe",
        Key::RetryCount => "reconnect attempts",
        Key::StopReconnect => "Stop reconnecting",
        Key::Reconnect => "Reconnect",
        Key::RetryRealtime => "Retry realtime connection",
        Key::Connected => "Realtime connected",
        Key::Connecting => "Connecting realtime…",
        Key::Child => "Child",
        Key::State => "State",
        Key::Outcome => "Outcome",
        Key::TraceUnavailable => "Trace: no association provided",
        Key::Parent => "Parent",
        Key::Collapse => "Collapse",
        Key::ViewPhases => "View phases",
        Key::LoadingPhases => "Loading phases…",
        Key::Error => "Error",
        Key::Cancelled => "Cancelled",
        Key::Interrupted => "Interrupted",
        Key::LoadMore => "Load more",
        Key::ViewBody => "View body",
        Key::Viewer => "Viewer",
        Key::LoadedBytes => "Loaded",
        Key::DownloadLoadedBody => "Download loaded body",
        Key::Copied => "Copied",
        Key::CopyFailed => "Copy failed",
        Key::CopyCompleteText => "Copy complete text",
        Key::BodyIncomplete => "Body is not fully loaded",
        Key::BodyLengthMismatch => "Loaded length does not match descriptor",
        Key::BodyCompleteHash => "Body complete · SHA-256 verified",
        Key::BodyHashMismatch => "Whole-body SHA-256 mismatch",
        Key::ViewChunks => "View chunks",
        Key::LoadMorePhases => "Load more phases",
        Key::BinaryChunk => "Binary chunk",
        Key::UndecodableChunk => "Chunk could not be decoded",
        Key::ChunkVerified => "verified",
        Key::ChunkInvalid => "failed verification",
        Key::PageHidden => "Page hidden; automatic reconnect paused",
        Key::AutoReconnectPaused => "Automatic reconnect paused",
        Key::SocketInterrupted => "Realtime WebSocket interrupted; reconnecting",
        Key::SocketDisconnected => "Realtime WebSocket disconnected; reconnecting",
        Key::SocketUnavailable => "Realtime WebSocket unavailable; reconnecting",
        Key::RetryLimit => "Automatic reconnect limit reached",
        Key::RetryWindowLimit => "Automatic reconnect time limit reached",
    }
}

#[cfg(test)]
mod tests {
    use super::{Copy, Key, Locale};

    #[test]
    fn locales_keep_the_same_key_surface() {
        let zh = Copy::for_locale(Locale::ZhCn);
        let en = Copy::en_us();
        assert_eq!(zh.text(Key::ToolProgram), "Tool Program");
        assert_eq!(en.text(Key::ToolProgram), "Tool Program");
        assert_ne!(zh.text(Key::Workbench), en.text(Key::Workbench));
    }
}
