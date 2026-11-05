//! Typed application-port adapters; no additional owner or event algorithm.
use crate::{ClientError, WorkClient, WorkspaceClient};
use futures_util::StreamExt;
use serde_json::Value;
use work_core::api::{
    AgentBackendOption, AuthState, AuthStatus, Capabilities, Diff, FileContent, FileListing,
    GitStatus, Health, History, HostResponse, ModelDescriptor, NodeApi, NodeError, NodeErrorKind,
    NodeEventStream, NodeFuture, Project, ProjectFolders, QueueCancellation, QueueMutation,
    QueueSnapshot, RuntimeState, SessionAccess, SessionAgentBinding, SessionBranch,
    SessionConfiguration, SessionMutation, SessionPreferences, SessionSummary, Snapshot,
    ToolCatalog,
};

impl NodeApi for WorkspaceClient {
    fn provider_catalog(&self) -> work_core::api::NodeFuture<'_, work_core::api::ProviderCatalog> {
        match self {
            Self::Remote(client) => NodeApi::provider_catalog(client),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::provider_catalog(client.as_ref()),
        }
    }
    fn provider_mutate(
        &self,
        request: work_core::api::ProviderMutation,
    ) -> work_core::api::NodeFuture<'_, work_core::api::ProviderReceipt> {
        match self {
            Self::Remote(client) => NodeApi::provider_mutate(client, request),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::provider_mutate(client.as_ref(), request),
        }
    }
    fn provider_release_hosts(
        &self,
    ) -> work_core::api::NodeFuture<'_, work_core::api::NativeReleaseReport> {
        match self {
            Self::Remote(client) => NodeApi::provider_release_hosts(client),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::provider_release_hosts(client.as_ref()),
        }
    }

    fn package_catalog<'a>(
        &'a self,
        project: &'a str,
    ) -> NodeFuture<'a, work_core::api::PackageCatalog> {
        match self {
            Self::Remote(client) => NodeApi::package_catalog(client, project),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::package_catalog(client.as_ref(), project),
        }
    }
    fn package_bundle<'a>(
        &'a self,
        project: &'a str,
        package: work_core::api::PackageRef,
    ) -> NodeFuture<'a, work_core::api::PackageBundle> {
        match self {
            Self::Remote(client) => NodeApi::package_bundle(client, project, package),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::package_bundle(client.as_ref(), project, package),
        }
    }
    fn package_acquire<'a>(
        &'a self,
        project: &'a str,
        package: work_core::api::PackageRef,
    ) -> NodeFuture<'a, work_core::api::PackageAcquisition> {
        match self {
            Self::Remote(client) => NodeApi::package_acquire(client, project, package),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::package_acquire(client.as_ref(), project, package),
        }
    }
    fn package_install<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::PackageInstall,
    ) -> NodeFuture<'a, work_core::api::NativeReceipt> {
        match self {
            Self::Remote(client) => NodeApi::package_install(client, project, request),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::package_install(client.as_ref(), project, request),
        }
    }
    fn native_catalog<'a>(
        &'a self,
        project: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeCatalog> {
        match self {
            Self::Remote(client) => NodeApi::native_catalog(client, project),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::native_catalog(client.as_ref(), project),
        }
    }
    fn native_manage<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::NativeManagement,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeReceipt> {
        match self {
            Self::Remote(client) => NodeApi::native_manage(client, project, request),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::native_manage(client.as_ref(), project, request),
        }
    }
    fn native_session_resources<'a>(
        &'a self,
        id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeSessionResources> {
        match self {
            Self::Remote(client) => NodeApi::native_session_resources(client, id),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::native_session_resources(client.as_ref(), id),
        }
    }
    fn native_apply<'a>(
        &'a self,
        id: &'a str,
        request: work_core::api::NativeApply,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeOperation> {
        match self {
            Self::Remote(client) => NodeApi::native_apply(client, id, request),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::native_apply(client.as_ref(), id, request),
        }
    }
    fn native_create<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::NativeCreate,
    ) -> work_core::api::NodeFuture<'a, String> {
        match self {
            Self::Remote(client) => NodeApi::native_create(client, project, request),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::native_create(client.as_ref(), project, request),
        }
    }
    fn native_release<'a>(
        &'a self,
        project: &'a str,
        scope: work_core::api::NativeScope,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeReleaseReport> {
        match self {
            Self::Remote(client) => NodeApi::native_release(client, project, scope),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::native_release(client.as_ref(), project, scope),
        }
    }
    fn native_saved_history<'a>(
        &'a self,
        id: &'a str,
        cursor: Option<String>,
    ) -> work_core::api::NodeFuture<'a, Value> {
        match self {
            Self::Remote(client) => NodeApi::native_saved_history(client, id, cursor),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::native_saved_history(client.as_ref(), id, cursor),
        }
    }
    fn authoring<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::AuthoringRequest,
    ) -> work_core::api::NodeFuture<'a, work_core::api::AuthoringState> {
        match self {
            Self::Remote(client) => NodeApi::authoring(client, project, request),
            #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
            Self::Local(client) => NodeApi::authoring(client.as_ref(), project, request),
        }
    }

    fn health(&self) -> NodeFuture<'_, Health> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::health(client.as_ref());
        }
        Box::pin(async move { WorkspaceClient::health(self).await.map_err(node_error) })
    }

    fn auth(&self) -> NodeFuture<'_, AuthState> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::auth(client.as_ref());
        }
        Box::pin(async move { WorkspaceClient::auth(self).await.map_err(node_error) })
    }

    fn login<'a>(&'a self, password: &'a str) -> NodeFuture<'a, AuthState> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::login(client.as_ref(), password);
        }
        Box::pin(async move {
            WorkspaceClient::login(self, password)
                .await
                .map_err(node_error)
        })
    }

    fn logout(&self) -> NodeFuture<'_, AuthState> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::logout(client.as_ref());
        }
        Box::pin(async move { WorkspaceClient::logout(self).await.map_err(node_error) })
    }

    fn capabilities(&self) -> NodeFuture<'_, Capabilities> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::capabilities(client.as_ref());
        }
        Box::pin(async move {
            WorkspaceClient::capabilities(self)
                .await
                .map_err(node_error)
        })
    }

    fn projects(&self) -> NodeFuture<'_, Vec<Project>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::projects(client.as_ref());
        }
        Box::pin(async move { WorkspaceClient::projects(self).await.map_err(node_error) })
    }

    fn rename_project<'a>(&'a self, project_id: &'a str, name: &'a str) -> NodeFuture<'a, Project> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::rename_project(client.as_ref(), project_id, name);
        }
        Box::pin(async move {
            WorkspaceClient::rename_project(self, project_id, name)
                .await
                .map_err(node_error)
        })
    }

    fn project_folders<'a>(&'a self, path: &'a str) -> NodeFuture<'a, ProjectFolders> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::project_folders(client.as_ref(), path);
        }
        Box::pin(async move {
            WorkspaceClient::project_folders(self, path)
                .await
                .map_err(node_error)
        })
    }

    fn create_project<'a>(
        &'a self,
        name: &'a str,
        path: Option<&'a str>,
        init_git: bool,
    ) -> NodeFuture<'a, Project> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::create_project(client.as_ref(), name, path, init_git);
        }
        Box::pin(async move {
            WorkspaceClient::create_project(self, name, path, init_git)
                .await
                .map_err(node_error)
        })
    }

    fn files<'a>(&'a self, project_id: &'a str, path: &'a str) -> NodeFuture<'a, FileListing> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::files(client.as_ref(), project_id, path);
        }
        Box::pin(async move {
            WorkspaceClient::files(self, project_id, path)
                .await
                .map_err(node_error)
        })
    }

    fn file<'a>(&'a self, project_id: &'a str, path: &'a str) -> NodeFuture<'a, FileContent> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::file(client.as_ref(), project_id, path);
        }
        Box::pin(async move {
            WorkspaceClient::file(self, project_id, path)
                .await
                .map_err(node_error)
        })
    }

    fn image<'a>(
        &'a self, project_id: &'a str, path: &'a str,
    ) -> NodeFuture<'a, work_core::api::ProjectImage> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::image(client.as_ref(), project_id, path);
        }
        Box::pin(async move {
            WorkspaceClient::image(self, project_id, path).await.map_err(node_error)
        })
    }

    fn git_status<'a>(&'a self, project_id: &'a str) -> NodeFuture<'a, GitStatus> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::git_status(client.as_ref(), project_id);
        }
        Box::pin(async move {
            WorkspaceClient::git_status(self, project_id)
                .await
                .map_err(node_error)
        })
    }

    fn diff<'a>(
        &'a self,
        project_id: &'a str,
        path: &'a str,
        staged: bool,
    ) -> NodeFuture<'a, Diff> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::diff(client.as_ref(), project_id, path, staged);
        }
        Box::pin(async move {
            WorkspaceClient::diff(self, project_id, path, staged)
                .await
                .map_err(node_error)
        })
    }

    fn agent_backends(&self) -> NodeFuture<'_, Vec<AgentBackendOption>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::agent_backends(client.as_ref());
        }
        Box::pin(async move {
            WorkspaceClient::agent_backends(self)
                .await
                .map_err(node_error)
        })
    }

    fn session_agent_binding<'a>(&'a self, id: &'a str) -> NodeFuture<'a, SessionAgentBinding> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::session_agent_binding(client.as_ref(), id);
        }
        Box::pin(async move {
            WorkspaceClient::session_agent_binding(self, id)
                .await
                .map_err(node_error)
        })
    }

    fn create_session_with_backend<'a>(
        &'a self,
        project_id: Option<&'a str>,
        backend: &'a str,
    ) -> NodeFuture<'a, String> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::create_session_with_backend(client.as_ref(), project_id, backend);
        }
        Box::pin(async move {
            WorkspaceClient::create_session_with_backend(self, project_id, backend)
                .await
                .map_err(node_error)
        })
    }

    fn sessions(&self) -> NodeFuture<'_, Vec<SessionSummary>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::sessions(client.as_ref());
        }
        Box::pin(async move { WorkspaceClient::sessions(self).await.map_err(node_error) })
    }

    fn create_session<'a>(&'a self, project_id: Option<&'a str>) -> NodeFuture<'a, String> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::create_session(client.as_ref(), project_id);
        }
        Box::pin(async move {
            WorkspaceClient::create_session(self, project_id)
                .await
                .map_err(node_error)
        })
    }

    fn get_session<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, SessionSummary> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::get_session(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::get_session(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn rename_session<'a>(
        &'a self,
        session_id: &'a str,
        title: &'a str,
    ) -> NodeFuture<'a, SessionMutation> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::rename_session(client.as_ref(), session_id, title);
        }
        Box::pin(async move {
            WorkspaceClient::rename_session(self, session_id, title)
                .await
                .map_err(node_error)
        })
    }

    fn session_preferences<'a>(
        &'a self,
        session_id: &'a str,
        archived: Option<bool>,
        pinned: Option<bool>,
    ) -> NodeFuture<'a, SessionPreferences> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::session_preferences(client.as_ref(), session_id, archived, pinned);
        }
        Box::pin(async move {
            WorkspaceClient::session_preferences(self, session_id, archived, pinned)
                .await
                .map_err(node_error)
        })
    }

    fn fork_session<'a>(
        &'a self,
        session_id: &'a str,
        entry_id: Option<&'a str>,
        title: Option<&'a str>,
    ) -> NodeFuture<'a, SessionMutation> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::fork_session(client.as_ref(), session_id, entry_id, title);
        }
        Box::pin(async move {
            WorkspaceClient::fork_session(self, session_id, entry_id, title)
                .await
                .map_err(node_error)
        })
    }

    fn clone_session<'a>(
        &'a self,
        session_id: &'a str,
        title: Option<&'a str>,
    ) -> NodeFuture<'a, SessionMutation> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::clone_session(client.as_ref(), session_id, title);
        }
        Box::pin(async move {
            WorkspaceClient::clone_session(self, session_id, title)
                .await
                .map_err(node_error)
        })
    }

    fn delete_session<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, ()> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::delete_session(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::delete_session(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn open_session<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, SessionSummary> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::open_session(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::open_session(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn snapshot<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Snapshot> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::snapshot(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::snapshot(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn snapshot_with_limit<'a>(
        &'a self,
        session_id: &'a str,
        limit: u64,
    ) -> NodeFuture<'a, Snapshot> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::snapshot_with_limit(client.as_ref(), session_id, limit);
        }
        Box::pin(async move {
            WorkspaceClient::snapshot_with_limit(self, session_id, limit)
                .await
                .map_err(node_error)
        })
    }

    fn session_access<'a>(&'a self, id: &'a str) -> NodeFuture<'a, SessionAccess> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::session_access(client.as_ref(), id);
        }
        Box::pin(async move {
            WorkspaceClient::session_access(self, id)
                .await
                .map_err(node_error)
        })
    }

    fn session_configuration<'a>(&'a self, id: &'a str) -> NodeFuture<'a, SessionConfiguration> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::session_configuration(client.as_ref(), id);
        }
        Box::pin(async move {
            WorkspaceClient::session_configuration(self, id)
                .await
                .map_err(node_error)
        })
    }

    fn set_session_config_option<'a>(
        &'a self,
        id: &'a str,
        config_id: &'a str,
        value: Value,
    ) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::set_session_config_option(client.as_ref(), id, config_id, value);
        }
        Box::pin(async move {
            WorkspaceClient::set_session_config_option(self, id, config_id, value)
                .await
                .map_err(node_error)
        })
    }

    fn session_branch<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, SessionBranch> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::session_branch(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::session_branch(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn history<'a>(
        &'a self,
        session_id: &'a str,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
        limit: u64,
    ) -> NodeFuture<'a, History> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::history(client.as_ref(), session_id, after_seq, before_seq, limit);
        }
        Box::pin(async move {
            WorkspaceClient::history(self, session_id, after_seq, before_seq, limit)
                .await
                .map_err(node_error)
        })
    }

    fn runtime_state<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, RuntimeState> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::runtime_state(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::runtime_state(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn active_tools_state<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Vec<String>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::active_tools_state(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::active_tools_state(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn models<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Vec<ModelDescriptor>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::models(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::models(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn auth_status<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, AuthStatus> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::auth_status(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::auth_status(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn thinking_levels<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Vec<String>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::thinking_levels(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::thinking_levels(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn tools<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, ToolCatalog> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::tools(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::tools(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn set_model<'a>(&'a self, session_id: &'a str, model_id: &'a str) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::set_model(client.as_ref(), session_id, model_id);
        }
        Box::pin(async move {
            WorkspaceClient::set_model(self, session_id, model_id)
                .await
                .map_err(node_error)
        })
    }

    fn set_thinking_level<'a>(
        &'a self,
        session_id: &'a str,
        level: &'a str,
    ) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::set_thinking_level(client.as_ref(), session_id, level);
        }
        Box::pin(async move {
            WorkspaceClient::set_thinking_level(self, session_id, level)
                .await
                .map_err(node_error)
        })
    }

    fn set_active_tools<'a>(
        &'a self,
        session_id: &'a str,
        tools: &'a [String],
    ) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::set_active_tools(client.as_ref(), session_id, tools);
        }
        Box::pin(async move {
            WorkspaceClient::set_active_tools(self, session_id, tools)
                .await
                .map_err(node_error)
        })
    }

    fn resume_pending<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::resume_pending(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::resume_pending(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn compact<'a>(
        &'a self,
        session_id: &'a str,
        custom_instructions: Option<&'a str>,
    ) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::compact(client.as_ref(), session_id, custom_instructions);
        }
        Box::pin(async move {
            WorkspaceClient::compact(self, session_id, custom_instructions)
                .await
                .map_err(node_error)
        })
    }

    fn queue<'a>(
        &'a self,
        session_id: &'a str,
        include_completed: bool,
    ) -> NodeFuture<'a, QueueSnapshot> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::queue(client.as_ref(), session_id, include_completed);
        }
        Box::pin(async move {
            WorkspaceClient::queue(self, session_id, include_completed)
                .await
                .map_err(node_error)
        })
    }

    fn queue_message<'a>(
        &'a self,
        session_id: &'a str,
        message: &'a str,
        deliver_as: &'a str,
    ) -> NodeFuture<'a, QueueMutation> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::queue_message(client.as_ref(), session_id, message, deliver_as);
        }
        Box::pin(async move {
            WorkspaceClient::queue_message(self, session_id, message, deliver_as)
                .await
                .map_err(node_error)
        })
    }

    fn cancel_queued<'a>(
        &'a self,
        session_id: &'a str,
        entry_id: &'a str,
    ) -> NodeFuture<'a, QueueCancellation> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::cancel_queued(client.as_ref(), session_id, entry_id);
        }
        Box::pin(async move {
            WorkspaceClient::cancel_queued(self, session_id, entry_id)
                .await
                .map_err(node_error)
        })
    }

    fn clear_queue<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::clear_queue(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::clear_queue(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn dialog_response<'a>(
        &'a self,
        session_id: &'a str,
        request_id: &'a str,
        value: Value,
    ) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::dialog_response(client.as_ref(), session_id, request_id, value);
        }
        Box::pin(async move {
            WorkspaceClient::dialog_response(self, session_id, request_id, value)
                .await
                .map_err(node_error)
        })
    }

    fn cancel_dialog<'a>(
        &'a self,
        session_id: &'a str,
        request_id: &'a str,
    ) -> NodeFuture<'a, Value> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::cancel_dialog(client.as_ref(), session_id, request_id);
        }
        Box::pin(async move {
            WorkspaceClient::cancel_dialog(self, session_id, request_id)
                .await
                .map_err(node_error)
        })
    }

    fn prompt<'a>(
        &'a self,
        session_id: &'a str,
        message: &'a str,
    ) -> NodeFuture<'a, HostResponse<Value>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::prompt(client.as_ref(), session_id, message);
        }
        Box::pin(async move {
            WorkspaceClient::prompt(self, session_id, message)
                .await
                .map_err(node_error)
        })
    }

    fn prompt_content<'a>(
        &'a self,
        session_id: &'a str,
        content_blocks: &'a Value,
    ) -> NodeFuture<'a, HostResponse<Value>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::prompt_content(client.as_ref(), session_id, content_blocks);
        }
        Box::pin(async move {
            WorkspaceClient::prompt_content(self, session_id, content_blocks).await.map_err(node_error)
        })
    }

    fn abort<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, HostResponse<Value>> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::abort(client.as_ref(), session_id);
        }
        Box::pin(async move {
            WorkspaceClient::abort(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn subscribe<'a>(&'a self, session_id: &'a str, after: u64) -> NodeFuture<'a, NodeEventStream> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        if let Self::Local(client) = self {
            return NodeApi::subscribe(client.as_ref(), session_id, after);
        }
        Box::pin(async move {
            WorkspaceClient::subscribe(self, session_id, after)
                .await
                .map_err(node_error)
                .map(|stream| {
                    Box::pin(stream.map(|item| item.map_err(node_error))) as NodeEventStream
                })
        })
    }
}

impl NodeApi for WorkClient {
    fn provider_catalog(&self) -> work_core::api::NodeFuture<'_, work_core::api::ProviderCatalog> {
        Box::pin(async move { WorkClient::provider_catalog(self).await.map_err(node_error) })
    }
    fn provider_mutate(
        &self,
        request: work_core::api::ProviderMutation,
    ) -> work_core::api::NodeFuture<'_, work_core::api::ProviderReceipt> {
        Box::pin(async move {
            WorkClient::provider_mutate(self, request)
                .await
                .map_err(node_error)
        })
    }
    fn provider_release_hosts(
        &self,
    ) -> work_core::api::NodeFuture<'_, work_core::api::NativeReleaseReport> {
        Box::pin(async move {
            WorkClient::provider_release_hosts(self)
                .await
                .map_err(node_error)
        })
    }

    fn package_catalog<'a>(
        &'a self,
        project: &'a str,
    ) -> NodeFuture<'a, work_core::api::PackageCatalog> {
        Box::pin(async move {
            WorkClient::package_catalog(self, project)
                .await
                .map_err(node_error)
        })
    }
    fn package_bundle<'a>(
        &'a self,
        project: &'a str,
        package: work_core::api::PackageRef,
    ) -> NodeFuture<'a, work_core::api::PackageBundle> {
        Box::pin(async move {
            WorkClient::package_bundle(self, project, package)
                .await
                .map_err(node_error)
        })
    }
    fn package_acquire<'a>(
        &'a self,
        project: &'a str,
        package: work_core::api::PackageRef,
    ) -> NodeFuture<'a, work_core::api::PackageAcquisition> {
        Box::pin(async move {
            WorkClient::package_acquire(self, project, package)
                .await
                .map_err(node_error)
        })
    }
    fn package_install<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::PackageInstall,
    ) -> NodeFuture<'a, work_core::api::NativeReceipt> {
        Box::pin(async move {
            WorkClient::package_install(self, project, request)
                .await
                .map_err(node_error)
        })
    }
    fn native_catalog<'a>(
        &'a self,
        project: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeCatalog> {
        Box::pin(async move {
            WorkClient::native_catalog(self, project)
                .await
                .map_err(node_error)
        })
    }
    fn native_manage<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::NativeManagement,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeReceipt> {
        Box::pin(async move {
            WorkClient::native_manage(self, project, request)
                .await
                .map_err(node_error)
        })
    }
    fn native_session_resources<'a>(
        &'a self,
        id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeSessionResources> {
        Box::pin(async move {
            WorkClient::native_session_resources(self, id)
                .await
                .map_err(node_error)
        })
    }
    fn native_apply<'a>(
        &'a self,
        id: &'a str,
        request: work_core::api::NativeApply,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeOperation> {
        Box::pin(async move {
            WorkClient::native_apply(self, id, request)
                .await
                .map_err(node_error)
        })
    }
    fn native_create<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::NativeCreate,
    ) -> work_core::api::NodeFuture<'a, String> {
        Box::pin(async move {
            WorkClient::native_create(self, project, request)
                .await
                .map_err(node_error)
        })
    }
    fn native_release<'a>(
        &'a self,
        project: &'a str,
        scope: work_core::api::NativeScope,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeReleaseReport> {
        Box::pin(async move {
            WorkClient::native_release(self, project, scope)
                .await
                .map_err(node_error)
        })
    }
    fn native_saved_history<'a>(
        &'a self,
        id: &'a str,
        cursor: Option<String>,
    ) -> work_core::api::NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::native_saved_history(self, id, cursor)
                .await
                .map_err(node_error)
        })
    }
    fn authoring<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::AuthoringRequest,
    ) -> work_core::api::NodeFuture<'a, work_core::api::AuthoringState> {
        Box::pin(async move {
            WorkClient::authoring(self, project, request)
                .await
                .map_err(node_error)
        })
    }

    fn health(&self) -> NodeFuture<'_, Health> {
        Box::pin(async move { WorkClient::health(self).await.map_err(node_error) })
    }

    fn auth(&self) -> NodeFuture<'_, AuthState> {
        Box::pin(async move { WorkClient::auth(self).await.map_err(node_error) })
    }

    fn login<'a>(&'a self, password: &'a str) -> NodeFuture<'a, AuthState> {
        Box::pin(async move { WorkClient::login(self, password).await.map_err(node_error) })
    }

    fn logout(&self) -> NodeFuture<'_, AuthState> {
        Box::pin(async move { WorkClient::logout(self).await.map_err(node_error) })
    }

    fn capabilities(&self) -> NodeFuture<'_, Capabilities> {
        Box::pin(async move { WorkClient::capabilities(self).await.map_err(node_error) })
    }

    fn projects(&self) -> NodeFuture<'_, Vec<Project>> {
        Box::pin(async move { WorkClient::projects(self).await.map_err(node_error) })
    }

    fn rename_project<'a>(&'a self, project_id: &'a str, name: &'a str) -> NodeFuture<'a, Project> {
        Box::pin(async move {
            WorkClient::rename_project(self, project_id, name)
                .await
                .map_err(node_error)
        })
    }

    fn project_folders<'a>(&'a self, path: &'a str) -> NodeFuture<'a, ProjectFolders> {
        Box::pin(async move {
            WorkClient::project_folders(self, path)
                .await
                .map_err(node_error)
        })
    }

    fn create_project<'a>(
        &'a self,
        name: &'a str,
        path: Option<&'a str>,
        init_git: bool,
    ) -> NodeFuture<'a, Project> {
        Box::pin(async move {
            WorkClient::create_project(self, name, path, init_git)
                .await
                .map_err(node_error)
        })
    }

    fn files<'a>(&'a self, project_id: &'a str, path: &'a str) -> NodeFuture<'a, FileListing> {
        Box::pin(async move {
            WorkClient::files(self, project_id, path)
                .await
                .map_err(node_error)
        })
    }

    fn file<'a>(&'a self, project_id: &'a str, path: &'a str) -> NodeFuture<'a, FileContent> {
        Box::pin(async move {
            WorkClient::file(self, project_id, path)
                .await
                .map_err(node_error)
        })
    }

    fn image<'a>(
        &'a self, project_id: &'a str, path: &'a str,
    ) -> NodeFuture<'a, work_core::api::ProjectImage> {
        Box::pin(async move { WorkClient::image(self, project_id, path).await.map_err(node_error) })
    }

    fn git_status<'a>(&'a self, project_id: &'a str) -> NodeFuture<'a, GitStatus> {
        Box::pin(async move {
            WorkClient::git_status(self, project_id)
                .await
                .map_err(node_error)
        })
    }

    fn diff<'a>(
        &'a self,
        project_id: &'a str,
        path: &'a str,
        staged: bool,
    ) -> NodeFuture<'a, Diff> {
        Box::pin(async move {
            WorkClient::diff(self, project_id, path, staged)
                .await
                .map_err(node_error)
        })
    }

    fn agent_backends(&self) -> NodeFuture<'_, Vec<AgentBackendOption>> {
        Box::pin(async move { WorkClient::agent_backends(self).await.map_err(node_error) })
    }

    fn session_agent_binding<'a>(&'a self, id: &'a str) -> NodeFuture<'a, SessionAgentBinding> {
        Box::pin(async move {
            WorkClient::session_agent_binding(self, id)
                .await
                .map_err(node_error)
        })
    }

    fn create_session_with_backend<'a>(
        &'a self,
        project_id: Option<&'a str>,
        backend: &'a str,
    ) -> NodeFuture<'a, String> {
        Box::pin(async move {
            WorkClient::create_session_with_backend(self, project_id, backend)
                .await
                .map_err(node_error)
        })
    }

    fn sessions(&self) -> NodeFuture<'_, Vec<SessionSummary>> {
        Box::pin(async move { WorkClient::sessions(self).await.map_err(node_error) })
    }

    fn create_session<'a>(&'a self, project_id: Option<&'a str>) -> NodeFuture<'a, String> {
        Box::pin(async move {
            WorkClient::create_session(self, project_id)
                .await
                .map_err(node_error)
        })
    }

    fn get_session<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, SessionSummary> {
        Box::pin(async move {
            WorkClient::get_session(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn rename_session<'a>(
        &'a self,
        session_id: &'a str,
        title: &'a str,
    ) -> NodeFuture<'a, SessionMutation> {
        Box::pin(async move {
            WorkClient::rename_session(self, session_id, title)
                .await
                .map_err(node_error)
        })
    }

    fn session_preferences<'a>(
        &'a self,
        session_id: &'a str,
        archived: Option<bool>,
        pinned: Option<bool>,
    ) -> NodeFuture<'a, SessionPreferences> {
        Box::pin(async move {
            WorkClient::session_preferences(self, session_id, archived, pinned)
                .await
                .map_err(node_error)
        })
    }

    fn fork_session<'a>(
        &'a self,
        session_id: &'a str,
        entry_id: Option<&'a str>,
        title: Option<&'a str>,
    ) -> NodeFuture<'a, SessionMutation> {
        Box::pin(async move {
            WorkClient::fork_session(self, session_id, entry_id, title)
                .await
                .map_err(node_error)
        })
    }

    fn clone_session<'a>(
        &'a self,
        session_id: &'a str,
        title: Option<&'a str>,
    ) -> NodeFuture<'a, SessionMutation> {
        Box::pin(async move {
            WorkClient::clone_session(self, session_id, title)
                .await
                .map_err(node_error)
        })
    }

    fn delete_session<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, ()> {
        Box::pin(async move {
            WorkClient::delete_session(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn open_session<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, SessionSummary> {
        Box::pin(async move {
            WorkClient::open_session(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn snapshot<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Snapshot> {
        Box::pin(async move {
            WorkClient::snapshot(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn snapshot_with_limit<'a>(
        &'a self,
        session_id: &'a str,
        limit: u64,
    ) -> NodeFuture<'a, Snapshot> {
        Box::pin(async move {
            WorkClient::snapshot_with_limit(self, session_id, limit)
                .await
                .map_err(node_error)
        })
    }

    fn session_access<'a>(&'a self, id: &'a str) -> NodeFuture<'a, SessionAccess> {
        Box::pin(async move {
            WorkClient::session_access(self, id)
                .await
                .map_err(node_error)
        })
    }

    fn session_configuration<'a>(&'a self, id: &'a str) -> NodeFuture<'a, SessionConfiguration> {
        Box::pin(async move {
            WorkClient::session_configuration(self, id)
                .await
                .map_err(node_error)
        })
    }

    fn set_session_config_option<'a>(
        &'a self,
        id: &'a str,
        config_id: &'a str,
        value: Value,
    ) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::set_session_config_option(self, id, config_id, value)
                .await
                .map_err(node_error)
        })
    }

    fn session_branch<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, SessionBranch> {
        Box::pin(async move {
            WorkClient::session_branch(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn history<'a>(
        &'a self,
        session_id: &'a str,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
        limit: u64,
    ) -> NodeFuture<'a, History> {
        Box::pin(async move {
            WorkClient::history(self, session_id, after_seq, before_seq, limit)
                .await
                .map_err(node_error)
        })
    }

    fn runtime_state<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, RuntimeState> {
        Box::pin(async move {
            WorkClient::runtime_state(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn active_tools_state<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Vec<String>> {
        Box::pin(async move {
            WorkClient::active_tools_state(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn models<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Vec<ModelDescriptor>> {
        Box::pin(async move {
            WorkClient::models(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn auth_status<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, AuthStatus> {
        Box::pin(async move {
            WorkClient::auth_status(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn thinking_levels<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Vec<String>> {
        Box::pin(async move {
            WorkClient::thinking_levels(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn tools<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, ToolCatalog> {
        Box::pin(async move {
            WorkClient::tools(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn set_model<'a>(&'a self, session_id: &'a str, model_id: &'a str) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::set_model(self, session_id, model_id)
                .await
                .map_err(node_error)
        })
    }

    fn set_thinking_level<'a>(
        &'a self,
        session_id: &'a str,
        level: &'a str,
    ) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::set_thinking_level(self, session_id, level)
                .await
                .map_err(node_error)
        })
    }

    fn set_active_tools<'a>(
        &'a self,
        session_id: &'a str,
        tools: &'a [String],
    ) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::set_active_tools(self, session_id, tools)
                .await
                .map_err(node_error)
        })
    }

    fn resume_pending<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::resume_pending(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn compact<'a>(
        &'a self,
        session_id: &'a str,
        custom_instructions: Option<&'a str>,
    ) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::compact(self, session_id, custom_instructions)
                .await
                .map_err(node_error)
        })
    }

    fn queue<'a>(
        &'a self,
        session_id: &'a str,
        include_completed: bool,
    ) -> NodeFuture<'a, QueueSnapshot> {
        Box::pin(async move {
            WorkClient::queue(self, session_id, include_completed)
                .await
                .map_err(node_error)
        })
    }

    fn queue_message<'a>(
        &'a self,
        session_id: &'a str,
        message: &'a str,
        deliver_as: &'a str,
    ) -> NodeFuture<'a, QueueMutation> {
        Box::pin(async move {
            WorkClient::queue_message(self, session_id, message, deliver_as)
                .await
                .map_err(node_error)
        })
    }

    fn cancel_queued<'a>(
        &'a self,
        session_id: &'a str,
        entry_id: &'a str,
    ) -> NodeFuture<'a, QueueCancellation> {
        Box::pin(async move {
            WorkClient::cancel_queued(self, session_id, entry_id)
                .await
                .map_err(node_error)
        })
    }

    fn clear_queue<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::clear_queue(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn dialog_response<'a>(
        &'a self,
        session_id: &'a str,
        request_id: &'a str,
        value: Value,
    ) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::dialog_response(self, session_id, request_id, value)
                .await
                .map_err(node_error)
        })
    }

    fn cancel_dialog<'a>(
        &'a self,
        session_id: &'a str,
        request_id: &'a str,
    ) -> NodeFuture<'a, Value> {
        Box::pin(async move {
            WorkClient::cancel_dialog(self, session_id, request_id)
                .await
                .map_err(node_error)
        })
    }

    fn prompt<'a>(
        &'a self,
        session_id: &'a str,
        message: &'a str,
    ) -> NodeFuture<'a, HostResponse<Value>> {
        Box::pin(async move {
            WorkClient::prompt(self, session_id, message)
                .await
                .map_err(node_error)
        })
    }

    fn prompt_content<'a>(
        &'a self,
        session_id: &'a str,
        content_blocks: &'a Value,
    ) -> NodeFuture<'a, HostResponse<Value>> {
        Box::pin(async move {
            WorkClient::prompt_content(self, session_id, content_blocks).await.map_err(node_error)
        })
    }

    fn abort<'a>(&'a self, session_id: &'a str) -> NodeFuture<'a, HostResponse<Value>> {
        Box::pin(async move {
            WorkClient::abort(self, session_id)
                .await
                .map_err(node_error)
        })
    }

    fn subscribe<'a>(&'a self, session_id: &'a str, after: u64) -> NodeFuture<'a, NodeEventStream> {
        Box::pin(async move {
            WorkClient::subscribe(self, session_id, after)
                .await
                .map_err(node_error)
                .map(|stream| {
                    Box::pin(stream.map(|item| item.map_err(node_error))) as NodeEventStream
                })
        })
    }
}

// Error bodies, URLs, paths and process diagnostics must not cross this port.
fn node_error(error: ClientError) -> NodeError {
    use NodeErrorKind as Kind;
    let kind = match error {
        ClientError::Api { status, .. } => match status.as_u16() {
            400 | 422 => Kind::InvalidInput,
            401 | 403 => Kind::AccessDenied,
            404 => Kind::NotFound,
            408 | 504 => Kind::Timeout,
            409 => Kind::Conflict,
            413 | 429 => Kind::LimitExceeded,
            415 => Kind::UnsupportedContent,
            501 => Kind::Unsupported,
            502 | 503 => Kind::Unavailable,
            _ => Kind::Internal,
        },
        ClientError::Unsupported { .. } => Kind::Unsupported,
        ClientError::RequestTimeout(_) => Kind::Timeout,
        ClientError::Http(error) if error.is_timeout() => Kind::Timeout,
        #[cfg(not(target_arch = "wasm32"))]
        ClientError::Http(error) if error.is_connect() => Kind::Unavailable,
        ClientError::ChildExited(_) => Kind::Unavailable,
        ClientError::InvalidBaseUrl(_) => Kind::InvalidInput,
        ClientError::AuthenticationNotConfirmed | ClientError::QrAuthorizationFailed => {
            Kind::AccessDenied
        }
        // Local WorkspaceClient calls take the owner's NodeApi path above, so
        // native service categories are retained without a lossy ClientError hop.
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        ClientError::Local(error) => match error {
            crate::LocalError::InvalidInput(_) => Kind::InvalidInput,
            crate::LocalError::NotFound(_) => Kind::NotFound,
            crate::LocalError::DuplicateProject(_) => Kind::Conflict,
            crate::LocalError::Unsupported(_) => Kind::Unsupported,
            crate::LocalError::Shutdown => Kind::Unavailable,
            _ => Kind::Internal,
        },
        _ => Kind::Internal,
    };
    let message = match kind {
        Kind::InvalidInput => "invalid node request",
        Kind::NotFound => "not found",
        Kind::AccessDenied => "node access denied",
        Kind::Conflict => "node operation conflicts with current state",
        Kind::Unsupported => "node operation unsupported",
        Kind::Unavailable => "node operation unavailable",
        Kind::LimitExceeded => "node request limit exceeded",
        Kind::UnsupportedContent => "node content unsupported",
        Kind::Timeout => "node operation timed out",
        Kind::Internal => "node operation failed",
    };
    NodeError::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{FutureExt, stream};

    #[test]
    fn http_categories_are_preserved_without_response_bodies() {
        for (status, kind) in [
            (400, NodeErrorKind::InvalidInput),
            (401, NodeErrorKind::AccessDenied),
            (403, NodeErrorKind::AccessDenied),
            (404, NodeErrorKind::NotFound),
            (408, NodeErrorKind::Timeout),
            (409, NodeErrorKind::Conflict),
            (413, NodeErrorKind::LimitExceeded),
            (415, NodeErrorKind::UnsupportedContent),
            (422, NodeErrorKind::InvalidInput),
            (429, NodeErrorKind::LimitExceeded),
            (500, NodeErrorKind::Internal),
            (501, NodeErrorKind::Unsupported),
            (502, NodeErrorKind::Unavailable),
            (503, NodeErrorKind::Unavailable),
            (504, NodeErrorKind::Timeout),
        ] {
            let error = node_error(ClientError::Api {
                status: reqwest::StatusCode::from_u16(status).unwrap(),
                message: "secret /private/path token".into(),
            });
            assert_eq!(error.kind, kind);
            assert!(!format!("{error:?} {error}").contains("secret"));
            assert!(!error.message.contains("/private/path"));
        }
    }

    #[test]
    fn transport_diagnostics_are_redacted() {
        for (error, expected) in [
            (
                ClientError::Unsupported {
                    operation: "secret-path".into(),
                    protocol: "secret-path".into(),
                },
                NodeErrorKind::Unsupported,
            ),
            (
                ClientError::RequestTimeout("secret-path".into()),
                NodeErrorKind::Timeout,
            ),
            (
                ClientError::ChildExited("secret-path".into()),
                NodeErrorKind::Unavailable,
            ),
            (
                ClientError::Process("secret-path".into()),
                NodeErrorKind::Internal,
            ),
            (
                ClientError::Sse("secret-path".into()),
                NodeErrorKind::Internal,
            ),
        ] {
            let error = node_error(error);
            assert_eq!(error.kind, expected);
            assert!(!format!("{error:?} {error}").contains("secret-path"));
        }
    }

    #[test]
    fn both_clients_expose_object_safe_ports() {
        let client = WorkClient::new("http://127.0.0.1:1").unwrap();
        let _: &dyn NodeApi = &client;
        let workspace = WorkspaceClient::Remote(client);
        let _: &dyn NodeApi = &workspace;
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn stream_mapping_preserves_events_order_and_errors() {
        use serde_json::json;
        use work_core::api::WireEvent;
        let event = |name: &str, seq: u64| WireEvent {
            id: Some(seq.to_string()),
            seq: Some(seq),
            event_type: name.into(),
            data: json!({"type": name}),
        };
        let ready = event("ready", 1_u64);
        let resync = event("resync_required", 2_u64);
        let replay = event("message", 3_u64);
        let source = stream::iter(vec![
            Ok(ready.clone()),
            Ok(resync.clone()),
            Ok(replay.clone()),
            Err(ClientError::RequestTimeout("secret".into())),
        ]);
        let mapped: NodeEventStream = Box::pin(source.map(|item| item.map_err(node_error)));
        let values = mapped.collect::<Vec<_>>().now_or_never().unwrap();
        assert_eq!(values[0].as_ref().unwrap(), &ready);
        assert_eq!(values[1].as_ref().unwrap(), &resync);
        assert_eq!(values[2].as_ref().unwrap(), &replay);
        assert_eq!(values[3].as_ref().unwrap_err().kind, NodeErrorKind::Timeout);
    }
}
