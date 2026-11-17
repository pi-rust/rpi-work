//! Native LAN discovery through the shared `rpi-node` mDNS provider.
//!
//! mDNS only supplies candidate HTTP origins. Every candidate is validated by
//! the existing credential-free `NodeProbe` before it is returned.

use std::collections::HashSet;

use rpi_node::{DiscoveryProvider, MDNS_SERVICE_TYPE, MdnsDiscoveryProvider};
use rpi_node_core::{DiscoveryObservation, NodeTransportKind};

use crate::{ClientError, NodeProbe};

#[derive(Debug, Clone)]
pub struct DiscoveredNode {
    pub endpoint: String,
    pub observation: DiscoveryObservation,
    pub probe: NodeProbe,
}

/// Browse the local network and return only nodes that pass the normal Work
/// discovery/health/protocol probe. Authentication is intentionally not part
/// of this operation.
///
/// A single unavailable or incompatible candidate does not hide other nodes;
/// only failure to start or browse the mDNS provider fails the operation.
pub async fn discover_nodes() -> Result<Vec<DiscoveredNode>, ClientError> {
    let provider = MdnsDiscoveryProvider::new().map_err(|error| {
        ClientError::ProtocolMismatch(format!("LAN discovery unavailable: {error}"))
    })?;
    let observations = provider
        .discover(MDNS_SERVICE_TYPE)
        .await
        .map_err(|error| ClientError::ProtocolMismatch(format!("LAN discovery failed: {error}")))?;

    let mut seen = HashSet::new();
    let mut nodes = Vec::new();
    for observation in observations {
        let Some(transport) = observation.transport.as_ref() else {
            continue;
        };
        if transport.kind != NodeTransportKind::Http {
            continue;
        }
        let Some(origin) = transport.origin.as_deref() else {
            continue;
        };
        let Ok(probe) = NodeProbe::probe(origin).await else {
            continue;
        };
        if observation
            .identity
            .as_ref()
            .is_some_and(|identity| identity.node_id != probe.node_id())
        {
            continue;
        }
        if !seen.insert(probe.node_id().to_owned()) {
            continue;
        }
        nodes.push(DiscoveredNode {
            endpoint: probe.endpoint.clone(),
            observation,
            probe,
        });
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use rpi_node_core::{DiscoveryObservation, DiscoverySource, NodeIdentity, NodeTransport};

    #[test]
    fn discovery_observation_keeps_locator_metadata_separate_from_authentication() {
        let observation = DiscoveryObservation {
            source: DiscoverySource::Mdns,
            transport: Some(NodeTransport::http("http://192.168.1.20:13031")),
            identity: Some(NodeIdentity {
                node_id: "node-1".into(),
                owner_epoch: "epoch-1".into(),
            }),
            manifest: None,
            external_descriptor: None,
        };
        assert!(observation.validate().is_ok());
        assert!(
            observation
                .transport
                .unwrap()
                .origin
                .unwrap()
                .contains("13031")
        );
    }
}
