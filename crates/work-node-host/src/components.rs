//! Bind a component to the existing owner; UI and action semantics live in its package.
use std::sync::Arc;
use work_node_runtime::workspace::LocalWorkspace;
struct RemoteAccess;
impl work_http_component::RemoteAccessOwner for RemoteAccess {
    fn snapshot(&self) -> work_http_component::Snapshot {
        let s = crate::http::sharing();
        let (configuration, restart_required) = crate::http::configuration();
        work_http_component::Snapshot {
            listening: s.listening,
            enabled: s.enabled,
            status: s.status,
            warning: s.warning,
            public_url: s.public_url,
            app_url: s.app_url,
            address: s.address,
            can_configure: false,
            address_hint: s.address_hint,
            node_id: s.node_id,
            web_hosted: s.web_hosted,
            configuration,
            restart_required,
        }
    }
    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        crate::http::set_shared(enabled).map(|_| ())
    }
    fn configure(&self, request: work_http_component::Configure) -> Result<(), String> {
        crate::http::configure(request)
    }
    fn issue_authorization(&self) -> Result<work_http_component::Authorization, String> {
        let grant = crate::http::issue_qr_authorization()?;
        Ok(work_http_component::Authorization {
            ticket: grant.ticket,
            expires_at: grant.expires_at,
            node_id: grant.node_id,
        })
    }
}
pub(crate) fn register(client: &Arc<LocalWorkspace>) -> Result<(), String> {
    work_http_component::register(client.component_host(), Arc::new(RemoteAccess))
}
