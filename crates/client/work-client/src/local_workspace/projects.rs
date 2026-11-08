use super::LocalWorkspace;
use crate::{ClientError, Diff, FileContent, FileListing, GitStatus, Project, ProjectFolders};
use std::path::Path;

impl LocalWorkspace {
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
    pub async fn image(
        &self,
        project_id: &str,
        path: &str,
    ) -> Result<crate::ProjectImage, ClientError> {
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
}
