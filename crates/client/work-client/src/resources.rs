//! Native resource/authoring wire adapter; authority remains at the node.
use crate::{ClientError, WorkClient};
use serde_json::{Value, json};
impl WorkClient {
    pub async fn package_catalog(
        &self,
        project: &str,
    ) -> Result<work_core::api::PackageCatalog, ClientError> {
        self.get(&format!("api/v1/projects/{project}/packages"))
            .await
    }
    pub async fn package_bundle(
        &self,
        project: &str,
        package: work_core::api::PackageRef,
    ) -> Result<work_core::api::PackageBundle, ClientError> {
        self.post(
            &format!("api/v1/projects/{project}/packages/bundle"),
            json!(package),
        )
        .await
    }
    pub async fn package_acquire(
        &self,
        project: &str,
        package: work_core::api::PackageRef,
    ) -> Result<work_core::api::PackageAcquisition, ClientError> {
        self.post(
            &format!("api/v1/projects/{project}/packages/acquire"),
            json!(package),
        )
        .await
    }
    pub async fn package_install(
        &self,
        project: &str,
        request: work_core::api::PackageInstall,
    ) -> Result<work_core::api::NativeReceipt, ClientError> {
        self.post(
            &format!("api/v1/projects/{project}/packages/install"),
            json!(request),
        )
        .await
    }
    pub async fn native_catalog(
        &self,
        project: &str,
    ) -> Result<work_core::api::NativeCatalog, ClientError> {
        self.get(&format!("api/v1/projects/{project}/native-resources"))
            .await
    }
    pub async fn native_project_resources(
        &self,
        project: &str,
    ) -> Result<work_core::api::NativeProjectResources, ClientError> {
        self.get(&format!("api/v1/projects/{project}/native-resource-config"))
            .await
    }
    pub async fn native_apply_project(
        &self,
        project: &str,
        request: work_core::api::NativeApply,
    ) -> Result<work_core::api::NativeOperation, ClientError> {
        self.post(
            &format!("api/v1/projects/{project}/native-resource-config"),
            json!(request),
        )
        .await
    }
    pub async fn native_manage(
        &self,
        project: &str,
        request: work_core::api::NativeManagement,
    ) -> Result<work_core::api::NativeReceipt, ClientError> {
        self.post(
            &format!("api/v1/projects/{project}/native-resources"),
            json!(request),
        )
        .await
    }
    pub async fn native_session_resources(
        &self,
        id: &str,
    ) -> Result<work_core::api::NativeSessionResources, ClientError> {
        self.get(&format!("api/v1/sessions/{id}/resources")).await
    }
    pub async fn native_apply(
        &self,
        id: &str,
        request: work_core::api::NativeApply,
    ) -> Result<work_core::api::NativeOperation, ClientError> {
        self.post(&format!("api/v1/sessions/{id}/resources"), json!(request))
            .await
    }
    pub async fn native_create(
        &self,
        project: &str,
        request: work_core::api::NativeCreate,
    ) -> Result<String, ClientError> {
        self.post::<Value>(
            &format!("api/v1/projects/{project}/resource-sessions"),
            json!(request),
        )
        .await
        .and_then(|v| {
            v["sessionId"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| ClientError::Protocol("missing created session identity".into()))
        })
    }
    pub async fn native_release(
        &self,
        project: &str,
        scope: work_core::api::NativeScope,
    ) -> Result<work_core::api::NativeReleaseReport, ClientError> {
        self.post(
            &format!("api/v1/projects/{project}/release-idle-resource-hosts"),
            json!({"scope":scope}),
        )
        .await
    }
    pub async fn native_saved_history(
        &self,
        id: &str,
        cursor: Option<String>,
    ) -> Result<Value, ClientError> {
        self.post(
            &format!("api/v1/sessions/{id}/saved-resource-history"),
            json!({"cursor":cursor}),
        )
        .await
    }
    pub async fn authoring(
        &self,
        project: &str,
        request: work_core::api::AuthoringRequest,
    ) -> Result<work_core::api::AuthoringState, ClientError> {
        self.post(
            &format!("api/v1/projects/{project}/authoring"),
            json!(request),
        )
        .await
    }
}
