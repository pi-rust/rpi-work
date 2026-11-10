//! Adapter from Work's public capability model to the transport-neutral node document.
//!
//! This module only describes the existing workspace owner. It does not create
//! another workspace, node host, authentication lease, or lifecycle guard.
use rpi_node_core::{
    CapabilityBinding, NodeDiscoveryDocument, NodeIdentity, NodeManifest, NodeRole, NodeTransport,
    ProtocolBinding, ProtocolId,
};
use work_node_api::api::Capabilities;

const WORK_PROTOCOL: &str = "rpi-work";
const WORK_VERSION: &str = "1";
const HTTP_PATH: &str = "/api/v1";
const REALTIME_PATH: &str = "/api/v1/realtime";

/// Build and validate the public discovery document for one existing owner.
///
/// The origin is derived from the already validated request Host header by the
/// route. It is a location hint only; authentication and the realtime Hello
/// frame remain authoritative for identity and authorization.
pub(crate) fn document(
    node_id: &str,
    owner_epoch: &str,
    origin: &str,
    capabilities: &Capabilities,
) -> Result<NodeDiscoveryDocument, String> {
    let identity = NodeIdentity {
        node_id: node_id.to_owned(),
        owner_epoch: owner_epoch.to_owned(),
    };
    identity.validate().map_err(|error| error.to_string())?;

    let mut websocket = NodeTransport::websocket(origin.to_owned(), REALTIME_PATH);
    websocket.subprotocol = Some(work_node_api::api::realtime::SUBPROTOCOL.to_owned());
    let bindings = vec![
        ProtocolBinding {
            protocol: ProtocolId::from(WORK_PROTOCOL),
            version: WORK_VERSION.to_owned(),
            transport: {
                let mut transport = NodeTransport::http(origin.to_owned());
                transport.path = Some(HTTP_PATH.to_owned());
                transport
            },
            descriptor: None,
        },
        ProtocolBinding {
            protocol: ProtocolId::from(WORK_PROTOCOL),
            version: WORK_VERSION.to_owned(),
            transport: websocket,
            descriptor: None,
        },
    ];

    let mut advertised = Vec::with_capacity(
        capabilities.supported_commands.len() + capabilities.supported_actions.len(),
    );
    for operation in &capabilities.supported_commands {
        advertised.push(CapabilityBinding {
            namespace: "commands".to_owned(),
            operation: operation.clone(),
            protocol: ProtocolId::from(WORK_PROTOCOL),
            version: Some(WORK_VERSION.to_owned()),
        });
    }
    for operation in &capabilities.supported_actions {
        advertised.push(CapabilityBinding {
            namespace: "actions".to_owned(),
            operation: operation.clone(),
            protocol: ProtocolId::from(WORK_PROTOCOL),
            version: Some(WORK_VERSION.to_owned()),
        });
    }

    let manifest = NodeManifest {
        extension_id: "rpi-work".to_owned(),
        extension_version: env!("CARGO_PKG_VERSION").to_owned(),
        protocol: rpi_node_core::PROTOCOL.into(),
        protocol_version: rpi_node_core::VERSION,
        roles: vec![NodeRole::Workspace, NodeRole::Agent],
        bindings: Vec::new(),
        capabilities: advertised,
    };
    let document = NodeDiscoveryDocument::from_manifest(&identity, manifest, bindings);
    document.validate().map_err(|error| error.to_string())?;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities() -> Capabilities {
        Capabilities {
            protocol: "rpi".into(),
            pi_compatibility_target: None,
            supported_commands: vec!["prompt".into(), "abort".into()],
            supported_actions: vec!["project.list".into()],
            semantics: None,
            limits: None,
            parity_complete: true,
        }
    }

    #[test]
    fn document_keeps_existing_owner_identity_and_work_bindings() {
        let document = document(
            "node-1",
            "epoch-1",
            "http://127.0.0.1:13031",
            &capabilities(),
        )
        .unwrap();

        assert_eq!(document.node_id, "node-1");
        assert_eq!(document.owner_epoch, "epoch-1");
        assert_eq!(document.bindings.len(), 2);
        assert!(document.bindings.iter().any(|binding| {
            binding.transport.path.as_deref() == Some(REALTIME_PATH)
                && binding.transport.subprotocol.as_deref()
                    == Some(work_node_api::api::realtime::SUBPROTOCOL)
        }));
        assert_eq!(document.capabilities.len(), 3);
        document.validate().unwrap();
    }

    #[test]
    fn document_rejects_invalid_identity_or_origin() {
        assert!(document("", "epoch-1", "http://node", &capabilities()).is_err());
        assert!(
            document(
                "node-1",
                "epoch-1",
                "http://user:password@node",
                &capabilities()
            )
            .is_err()
        );
    }
}
