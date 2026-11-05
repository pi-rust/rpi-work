//! Local access facade: owns no workspace state, process or client projection.
use crate::local_workspace_adapter::{into_agent_client, map_workspace_stream};
use crate::{
    AuthState, AuthStatus, Capabilities, ClientError, Diff, EventStream, FileContent, FileListing,
    GitStatus, Health, History, HostResponse, ModelDescriptor, Project, ProjectFolders,
    QueueCancellation, QueueMutation, QueueSnapshot, RuntimeState, SessionBranch,
    SessionConnection, SessionMutation, SessionPreferences, SessionSummary, Snapshot, ToolCatalog,
    WorkspaceClient,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use work_core::AgentDescriptor;
pub use work_node::workspace::LocalError;
use work_node::workspace::LocalWorkspace as NodeWorkspace;

/// Cloneable local access to the same node workspace used by the HTTP server.
#[derive(Clone)]
pub struct LocalWorkspace {
    owner: Arc<NodeWorkspace>,
}
impl LocalWorkspace {
    /// Wrap an existing owner without initializing another workspace or host.
    #[must_use]
    pub fn from_node_owner(owner: Arc<NodeWorkspace>) -> Self {
        Self { owner }
    }
    /// Share the exact owner with other node access adapters.
    #[must_use]
    pub fn node_owner(&self) -> Arc<NodeWorkspace> {
        self.owner.clone()
    }
    pub fn new(executable: PathBuf, data: PathBuf) -> Result<Self, ClientError> {
        NodeWorkspace::new(executable, data)
            .map(|owner| Self::from_node_owner(Arc::new(owner)))
            .map_err(Into::into)
    }
    pub fn with_session_dir(
        executable: PathBuf,
        data: PathBuf,
        session_dir: Option<PathBuf>,
    ) -> Result<Self, ClientError> {
        NodeWorkspace::with_session_dir(executable, data, session_dir)
            .map(|owner| Self::from_node_owner(Arc::new(owner)))
            .map_err(Into::into)
    }
    #[must_use]
    pub fn descriptor(&self) -> AgentDescriptor {
        self.owner.descriptor()
    }
    pub async fn health(&self) -> Result<Health, ClientError> {
        self.owner.health().await.map_err(Into::into)
    }
    pub async fn auth(&self) -> Result<AuthState, ClientError> {
        self.owner.auth().await.map_err(Into::into)
    }
    pub async fn login(&self, password: &str) -> Result<AuthState, ClientError> {
        self.owner.login(password).await.map_err(Into::into)
    }
    pub async fn logout(&self) -> Result<AuthState, ClientError> {
        self.owner.logout().await.map_err(Into::into)
    }
    pub async fn capabilities(&self) -> Result<Capabilities, ClientError> {
        self.owner.capabilities().await.map_err(Into::into)
    }
    pub async fn projects(&self) -> Result<Vec<Project>, ClientError> {
        self.owner.projects().await.map_err(Into::into)
    }
    pub async fn create_project(
        &self,
        name: &str,
        path: Option<&str>,
        init_git: bool,
    ) -> Result<Project, ClientError> {
        self.owner
            .create_project(name, path, init_git)
            .await
            .map_err(Into::into)
    }
    pub async fn create_project_directory(
        &self,
        name: &str,
        path: &Path,
        init_git: bool,
    ) -> Result<Project, ClientError> {
        self.owner
            .create_project_directory(name, path, init_git)
            .await
            .map_err(Into::into)
    }
    pub async fn rename_project(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<Project, ClientError> {
        self.owner
            .rename_project(project_id, name)
            .await
            .map_err(Into::into)
    }
    pub async fn project_folders(&self, path: &str) -> Result<ProjectFolders, ClientError> {
        self.owner.project_folders(path).await.map_err(Into::into)
    }
    pub async fn files(&self, project_id: &str, path: &str) -> Result<FileListing, ClientError> {
        self.owner.files(project_id, path).await.map_err(Into::into)
    }
    pub async fn file(&self, project_id: &str, path: &str) -> Result<FileContent, ClientError> {
        self.owner.file(project_id, path).await.map_err(Into::into)
    }
    pub async fn image(&self, project_id: &str, path: &str) -> Result<crate::ProjectImage, ClientError> {
        self.owner.image(project_id, path).await.map_err(Into::into)
    }
    pub async fn git_status(&self, project_id: &str) -> Result<GitStatus, ClientError> {
        self.owner.git_status(project_id).await.map_err(Into::into)
    }
    pub async fn diff(
        &self,
        project_id: &str,
        path: &str,
        staged: bool,
    ) -> Result<Diff, ClientError> {
        self.owner
            .diff(project_id, path, staged)
            .await
            .map_err(Into::into)
    }
    pub async fn agent_backends(&self) -> Result<Vec<crate::AgentBackendOption>, ClientError> {
        self.owner.agent_backends().await.map_err(Into::into)
    }
    pub async fn session_agent_binding(
        &self,
        id: &str,
    ) -> Result<crate::SessionAgentBinding, ClientError> {
        self.owner
            .session_agent_binding(id)
            .await
            .map_err(Into::into)
    }
    pub async fn create_session_with_backend(
        &self,
        project: Option<&str>,
        backend: &str,
    ) -> Result<String, ClientError> {
        self.owner
            .create_session_with_backend(project, backend)
            .await
            .map_err(Into::into)
    }
    pub async fn prepare_agent_session(
        &self,
        id: &str,
    ) -> Result<Option<crate::AgentClient>, ClientError> {
        self.owner
            .prepare_agent_session(id)
            .await
            .map(|agent| agent.map(into_agent_client))
            .map_err(Into::into)
    }
    pub async fn sessions(&self) -> Result<Vec<SessionSummary>, ClientError> {
        self.owner.sessions().await.map_err(Into::into)
    }
    pub async fn create_session(&self, project_id: Option<&str>) -> Result<String, ClientError> {
        self.owner
            .create_session(project_id)
            .await
            .map_err(Into::into)
    }
    pub async fn control_session(&self) -> Result<String, ClientError> {
        self.owner.control_session().await.map_err(Into::into)
    }
    pub async fn get_session(&self, session_id: &str) -> Result<SessionSummary, ClientError> {
        self.owner.get_session(session_id).await.map_err(Into::into)
    }
    pub async fn rename_session(
        &self,
        session_id: &str,
        title: &str,
    ) -> Result<SessionMutation, ClientError> {
        self.owner
            .rename_session(session_id, title)
            .await
            .map_err(Into::into)
    }
    pub async fn session_preferences(
        &self,
        session_id: &str,
        archived: Option<bool>,
        pinned: Option<bool>,
    ) -> Result<SessionPreferences, ClientError> {
        self.owner
            .session_preferences(session_id, archived, pinned)
            .await
            .map_err(Into::into)
    }
    pub async fn fork_session(
        &self,
        session_id: &str,
        entry_id: Option<&str>,
        title: Option<&str>,
    ) -> Result<SessionMutation, ClientError> {
        self.owner
            .fork_session(session_id, entry_id, title)
            .await
            .map_err(Into::into)
    }
    pub async fn clone_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionMutation, ClientError> {
        self.owner
            .clone_session(session_id, title)
            .await
            .map_err(Into::into)
    }
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ClientError> {
        self.owner
            .delete_session(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn open_session(&self, session_id: &str) -> Result<SessionSummary, ClientError> {
        self.owner
            .open_session(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn snapshot(&self, session_id: &str) -> Result<Snapshot, ClientError> {
        self.owner.snapshot(session_id).await.map_err(Into::into)
    }
    pub async fn snapshot_with_limit(
        &self,
        session_id: &str,
        limit: u64,
    ) -> Result<Snapshot, ClientError> {
        self.owner
            .snapshot_with_limit(session_id, limit)
            .await
            .map_err(Into::into)
    }
    pub async fn command<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        payload: Value,
    ) -> Result<HostResponse<T>, ClientError> {
        self.owner
            .command(session_id, payload)
            .await
            .map_err(Into::into)
    }
    pub async fn command_data<T: for<'de> Deserialize<'de>>(
        &self,
        session_id: &str,
        command: &str,
        payload: Value,
    ) -> Result<T, ClientError> {
        self.owner
            .command_data(session_id, command, payload)
            .await
            .map_err(Into::into)
    }
    pub async fn refresh_auth(&self, provider: &str) -> Result<Value, ClientError> {
        self.owner.refresh_auth(provider).await.map_err(Into::into)
    }
    pub async fn subscribe(
        &self,
        session_id: &str,
        after: u64,
    ) -> Result<EventStream, ClientError> {
        self.owner
            .subscribe(session_id, after)
            .await
            .map(map_workspace_stream)
            .map_err(Into::into)
    }
    pub async fn connect_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<SessionConnection, ClientError> {
        let session_id = session_id.into();
        WorkspaceClient::Local(Arc::new(self.clone()))
            .connect_session(session_id)
            .await
    }
    pub async fn suspend(&self) {
        self.owner.suspend().await;
    }
    pub async fn shutdown(&self) {
        self.owner.shutdown().await;
    }
    pub async fn session_branch(&self, session_id: &str) -> Result<SessionBranch, ClientError> {
        self.owner
            .session_branch(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn history(
        &self,
        session_id: &str,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
        limit: u64,
    ) -> Result<History, ClientError> {
        self.owner
            .history(session_id, after_seq, before_seq, limit)
            .await
            .map_err(Into::into)
    }
    pub async fn runtime_state(&self, session_id: &str) -> Result<RuntimeState, ClientError> {
        self.owner
            .runtime_state(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn active_tools_state(&self, session_id: &str) -> Result<Vec<String>, ClientError> {
        self.owner
            .active_tools_state(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn models(&self, session_id: &str) -> Result<Vec<ModelDescriptor>, ClientError> {
        self.owner.models(session_id).await.map_err(Into::into)
    }
    pub async fn auth_status(&self, session_id: &str) -> Result<AuthStatus, ClientError> {
        self.owner.auth_status(session_id).await.map_err(Into::into)
    }
    pub async fn thinking_levels(&self, session_id: &str) -> Result<Vec<String>, ClientError> {
        self.owner
            .thinking_levels(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn tools(&self, session_id: &str) -> Result<ToolCatalog, ClientError> {
        self.owner.tools(session_id).await.map_err(Into::into)
    }
    pub async fn set_model(&self, session_id: &str, model_id: &str) -> Result<Value, ClientError> {
        self.owner
            .set_model(session_id, model_id)
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
impl work_core::api::NodeApi for LocalWorkspace {
    fn provider_catalog(&self) -> work_core::api::NodeFuture<'_, work_core::api::ProviderCatalog> {
        work_core::api::NodeApi::provider_catalog(self.owner.as_ref())
    }
    fn provider_mutate(
        &self,
        request: work_core::api::ProviderMutation,
    ) -> work_core::api::NodeFuture<'_, work_core::api::ProviderReceipt> {
        work_core::api::NodeApi::provider_mutate(self.owner.as_ref(), request)
    }
    fn provider_release_hosts(
        &self,
    ) -> work_core::api::NodeFuture<'_, work_core::api::NativeReleaseReport> {
        work_core::api::NodeApi::provider_release_hosts(self.owner.as_ref())
    }

    fn package_catalog<'a>(
        &'a self,
        project: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::PackageCatalog> {
        work_core::api::NodeApi::package_catalog(self.owner.as_ref(), project)
    }
    fn package_bundle<'a>(
        &'a self,
        project: &'a str,
        package: work_core::api::PackageRef,
    ) -> work_core::api::NodeFuture<'a, work_core::api::PackageBundle> {
        work_core::api::NodeApi::package_bundle(self.owner.as_ref(), project, package)
    }
    fn package_acquire<'a>(
        &'a self,
        project: &'a str,
        package: work_core::api::PackageRef,
    ) -> work_core::api::NodeFuture<'a, work_core::api::PackageAcquisition> {
        work_core::api::NodeApi::package_acquire(self.owner.as_ref(), project, package)
    }
    fn package_install<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::PackageInstall,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeReceipt> {
        work_core::api::NodeApi::package_install(self.owner.as_ref(), project, request)
    }
    fn native_catalog<'a>(
        &'a self,
        project: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeCatalog> {
        work_core::api::NodeApi::native_catalog(self.owner.as_ref(), project)
    }
    fn native_manage<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::NativeManagement,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeReceipt> {
        work_core::api::NodeApi::native_manage(self.owner.as_ref(), project, request)
    }
    fn native_session_resources<'a>(
        &'a self,
        id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeSessionResources> {
        work_core::api::NodeApi::native_session_resources(self.owner.as_ref(), id)
    }
    fn native_apply<'a>(
        &'a self,
        id: &'a str,
        request: work_core::api::NativeApply,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeOperation> {
        work_core::api::NodeApi::native_apply(self.owner.as_ref(), id, request)
    }
    fn native_create<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::NativeCreate,
    ) -> work_core::api::NodeFuture<'a, String> {
        work_core::api::NodeApi::native_create(self.owner.as_ref(), project, request)
    }
    fn native_release<'a>(
        &'a self,
        project: &'a str,
        scope: work_core::api::NativeScope,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NativeReleaseReport> {
        work_core::api::NodeApi::native_release(self.owner.as_ref(), project, scope)
    }
    fn native_saved_history<'a>(
        &'a self,
        id: &'a str,
        cursor: Option<String>,
    ) -> work_core::api::NodeFuture<'a, Value> {
        work_core::api::NodeApi::native_saved_history(self.owner.as_ref(), id, cursor)
    }
    fn authoring<'a>(
        &'a self,
        project: &'a str,
        request: work_core::api::AuthoringRequest,
    ) -> work_core::api::NodeFuture<'a, work_core::api::AuthoringState> {
        work_core::api::NodeApi::authoring(self.owner.as_ref(), project, request)
    }

    fn health(&self) -> work_core::api::NodeFuture<'_, work_core::api::Health> {
        work_core::api::NodeApi::health(self.owner.as_ref())
    }

    fn auth(&self) -> work_core::api::NodeFuture<'_, work_core::api::AuthState> {
        work_core::api::NodeApi::auth(self.owner.as_ref())
    }

    fn login<'a>(
        &'a self,
        password: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::AuthState> {
        work_core::api::NodeApi::login(self.owner.as_ref(), password)
    }

    fn logout(&self) -> work_core::api::NodeFuture<'_, work_core::api::AuthState> {
        work_core::api::NodeApi::logout(self.owner.as_ref())
    }

    fn capabilities(&self) -> work_core::api::NodeFuture<'_, work_core::api::Capabilities> {
        work_core::api::NodeApi::capabilities(self.owner.as_ref())
    }

    fn projects(&self) -> work_core::api::NodeFuture<'_, Vec<work_core::api::Project>> {
        work_core::api::NodeApi::projects(self.owner.as_ref())
    }

    fn rename_project<'a>(
        &'a self,
        project_id: &'a str,
        name: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::Project> {
        work_core::api::NodeApi::rename_project(self.owner.as_ref(), project_id, name)
    }

    fn project_folders<'a>(
        &'a self,
        path: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::ProjectFolders> {
        work_core::api::NodeApi::project_folders(self.owner.as_ref(), path)
    }

    fn create_project<'a>(
        &'a self,
        name: &'a str,
        path: Option<&'a str>,
        init_git: bool,
    ) -> work_core::api::NodeFuture<'a, work_core::api::Project> {
        work_core::api::NodeApi::create_project(self.owner.as_ref(), name, path, init_git)
    }

    fn files<'a>(
        &'a self,
        project_id: &'a str,
        path: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::FileListing> {
        work_core::api::NodeApi::files(self.owner.as_ref(), project_id, path)
    }

    fn file<'a>(
        &'a self,
        project_id: &'a str,
        path: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::FileContent> {
        work_core::api::NodeApi::file(self.owner.as_ref(), project_id, path)
    }

    fn image<'a>(
        &'a self, project_id: &'a str, path: &'a str,
    ) -> work_core::api::NodeFuture<'a, crate::ProjectImage> {
        work_core::api::NodeApi::image(self.owner.as_ref(), project_id, path)
    }

    fn git_status<'a>(
        &'a self,
        project_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::GitStatus> {
        work_core::api::NodeApi::git_status(self.owner.as_ref(), project_id)
    }

    fn diff<'a>(
        &'a self,
        project_id: &'a str,
        path: &'a str,
        staged: bool,
    ) -> work_core::api::NodeFuture<'a, work_core::api::Diff> {
        work_core::api::NodeApi::diff(self.owner.as_ref(), project_id, path, staged)
    }

    fn agent_backends(
        &self,
    ) -> work_core::api::NodeFuture<'_, Vec<work_core::api::AgentBackendOption>> {
        work_core::api::NodeApi::agent_backends(self.owner.as_ref())
    }

    fn session_agent_binding<'a>(
        &'a self,
        id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionAgentBinding> {
        work_core::api::NodeApi::session_agent_binding(self.owner.as_ref(), id)
    }

    fn create_session_with_backend<'a>(
        &'a self,
        project_id: Option<&'a str>,
        backend: &'a str,
    ) -> work_core::api::NodeFuture<'a, String> {
        work_core::api::NodeApi::create_session_with_backend(
            self.owner.as_ref(),
            project_id,
            backend,
        )
    }

    fn sessions(&self) -> work_core::api::NodeFuture<'_, Vec<work_core::api::SessionSummary>> {
        work_core::api::NodeApi::sessions(self.owner.as_ref())
    }

    fn create_session<'a>(
        &'a self,
        project_id: Option<&'a str>,
    ) -> work_core::api::NodeFuture<'a, String> {
        work_core::api::NodeApi::create_session(self.owner.as_ref(), project_id)
    }

    fn get_session<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionSummary> {
        work_core::api::NodeApi::get_session(self.owner.as_ref(), session_id)
    }

    fn rename_session<'a>(
        &'a self,
        session_id: &'a str,
        title: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionMutation> {
        work_core::api::NodeApi::rename_session(self.owner.as_ref(), session_id, title)
    }

    fn session_preferences<'a>(
        &'a self,
        session_id: &'a str,
        archived: Option<bool>,
        pinned: Option<bool>,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionPreferences> {
        work_core::api::NodeApi::session_preferences(
            self.owner.as_ref(),
            session_id,
            archived,
            pinned,
        )
    }

    fn fork_session<'a>(
        &'a self,
        session_id: &'a str,
        entry_id: Option<&'a str>,
        title: Option<&'a str>,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionMutation> {
        work_core::api::NodeApi::fork_session(self.owner.as_ref(), session_id, entry_id, title)
    }

    fn clone_session<'a>(
        &'a self,
        session_id: &'a str,
        title: Option<&'a str>,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionMutation> {
        work_core::api::NodeApi::clone_session(self.owner.as_ref(), session_id, title)
    }

    fn delete_session<'a>(&'a self, session_id: &'a str) -> work_core::api::NodeFuture<'a, ()> {
        work_core::api::NodeApi::delete_session(self.owner.as_ref(), session_id)
    }

    fn open_session<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionSummary> {
        work_core::api::NodeApi::open_session(self.owner.as_ref(), session_id)
    }

    fn snapshot<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::Snapshot> {
        work_core::api::NodeApi::snapshot(self.owner.as_ref(), session_id)
    }

    fn snapshot_with_limit<'a>(
        &'a self,
        session_id: &'a str,
        limit: u64,
    ) -> work_core::api::NodeFuture<'a, work_core::api::Snapshot> {
        work_core::api::NodeApi::snapshot_with_limit(self.owner.as_ref(), session_id, limit)
    }

    fn session_access<'a>(
        &'a self,
        id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionAccess> {
        work_core::api::NodeApi::session_access(self.owner.as_ref(), id)
    }

    fn session_configuration<'a>(
        &'a self,
        id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionConfiguration> {
        work_core::api::NodeApi::session_configuration(self.owner.as_ref(), id)
    }

    fn set_session_config_option<'a>(
        &'a self,
        id: &'a str,
        config_id: &'a str,
        value: serde_json::Value,
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::set_session_config_option(
            self.owner.as_ref(),
            id,
            config_id,
            value,
        )
    }

    fn session_branch<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::SessionBranch> {
        work_core::api::NodeApi::session_branch(self.owner.as_ref(), session_id)
    }

    fn history<'a>(
        &'a self,
        session_id: &'a str,
        after_seq: Option<u64>,
        before_seq: Option<u64>,
        limit: u64,
    ) -> work_core::api::NodeFuture<'a, work_core::api::History> {
        work_core::api::NodeApi::history(
            self.owner.as_ref(),
            session_id,
            after_seq,
            before_seq,
            limit,
        )
    }

    fn runtime_state<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::RuntimeState> {
        work_core::api::NodeApi::runtime_state(self.owner.as_ref(), session_id)
    }

    fn active_tools_state<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, Vec<String>> {
        work_core::api::NodeApi::active_tools_state(self.owner.as_ref(), session_id)
    }

    fn models<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, Vec<work_core::api::ModelDescriptor>> {
        work_core::api::NodeApi::models(self.owner.as_ref(), session_id)
    }

    fn auth_status<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::AuthStatus> {
        work_core::api::NodeApi::auth_status(self.owner.as_ref(), session_id)
    }

    fn thinking_levels<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, Vec<String>> {
        work_core::api::NodeApi::thinking_levels(self.owner.as_ref(), session_id)
    }

    fn tools<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::ToolCatalog> {
        work_core::api::NodeApi::tools(self.owner.as_ref(), session_id)
    }

    fn set_model<'a>(
        &'a self,
        session_id: &'a str,
        model_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::set_model(self.owner.as_ref(), session_id, model_id)
    }

    fn set_thinking_level<'a>(
        &'a self,
        session_id: &'a str,
        level: &'a str,
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::set_thinking_level(self.owner.as_ref(), session_id, level)
    }

    fn set_active_tools<'a>(
        &'a self,
        session_id: &'a str,
        tools: &'a [String],
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::set_active_tools(self.owner.as_ref(), session_id, tools)
    }

    fn resume_pending<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::resume_pending(self.owner.as_ref(), session_id)
    }

    fn compact<'a>(
        &'a self,
        session_id: &'a str,
        custom_instructions: Option<&'a str>,
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::compact(self.owner.as_ref(), session_id, custom_instructions)
    }

    fn queue<'a>(
        &'a self,
        session_id: &'a str,
        include_completed: bool,
    ) -> work_core::api::NodeFuture<'a, work_core::api::QueueSnapshot> {
        work_core::api::NodeApi::queue(self.owner.as_ref(), session_id, include_completed)
    }

    fn queue_message<'a>(
        &'a self,
        session_id: &'a str,
        message: &'a str,
        deliver_as: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::QueueMutation> {
        work_core::api::NodeApi::queue_message(self.owner.as_ref(), session_id, message, deliver_as)
    }

    fn cancel_queued<'a>(
        &'a self,
        session_id: &'a str,
        entry_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::QueueCancellation> {
        work_core::api::NodeApi::cancel_queued(self.owner.as_ref(), session_id, entry_id)
    }

    fn clear_queue<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::clear_queue(self.owner.as_ref(), session_id)
    }

    fn dialog_response<'a>(
        &'a self,
        session_id: &'a str,
        request_id: &'a str,
        value: serde_json::Value,
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::dialog_response(self.owner.as_ref(), session_id, request_id, value)
    }

    fn cancel_dialog<'a>(
        &'a self,
        session_id: &'a str,
        request_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, serde_json::Value> {
        work_core::api::NodeApi::cancel_dialog(self.owner.as_ref(), session_id, request_id)
    }

    fn prompt<'a>(
        &'a self,
        session_id: &'a str,
        message: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::HostResponse<serde_json::Value>> {
        work_core::api::NodeApi::prompt(self.owner.as_ref(), session_id, message)
    }

    fn prompt_content<'a>(
        &'a self,
        session_id: &'a str,
        content_blocks: &'a Value,
    ) -> work_core::api::NodeFuture<'a, work_core::api::HostResponse<Value>> {
        work_core::api::NodeApi::prompt_content(self.owner.as_ref(), session_id, content_blocks)
    }

    fn abort<'a>(
        &'a self,
        session_id: &'a str,
    ) -> work_core::api::NodeFuture<'a, work_core::api::HostResponse<serde_json::Value>> {
        work_core::api::NodeApi::abort(self.owner.as_ref(), session_id)
    }

    fn subscribe<'a>(
        &'a self,
        session_id: &'a str,
        after: u64,
    ) -> work_core::api::NodeFuture<'a, work_core::api::NodeEventStream> {
        work_core::api::NodeApi::subscribe(self.owner.as_ref(), session_id, after)
    }
}
