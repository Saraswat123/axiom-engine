use anyhow::Result;
use serde::{Deserialize, Serialize};

/// P2P transport layer — Phase 6.
/// libp2p swarm with Kademlia DHT + gossipsub for:
///   - proof artifact broadcast across nodes
///   - distributed ZK prover coordination
///   - peer discovery for axiom-engine cluster
///
/// Uncomment libp2p dependency in Cargo.toml when Phase 6 begins.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerMessage {
    pub from: String,  // PeerId as string
    pub topic: MessageTopic,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageTopic {
    ProofArtifact,    // broadcast new ZK receipt
    ProveRequest,     // request distributed proof generation
    TraceGossip,      // share execution trace hash
    InvariantUpdate,  // updated system invariant
}

/// Transport trait — swappable: stdio | HTTP | libp2p
pub trait Transport: Send + Sync {
    fn broadcast(&self, msg: PeerMessage) -> impl std::future::Future<Output = Result<()>> + Send;
    fn subscribe(&self, topic: MessageTopic) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Stub — wire libp2p in Phase 6
pub struct StubTransport;

impl Transport for StubTransport {
    async fn broadcast(&self, msg: PeerMessage) -> Result<()> {
        tracing::info!(topic = ?msg.topic, "p2p broadcast stub — Phase 6");
        Ok(())
    }
    async fn subscribe(&self, topic: MessageTopic) -> Result<()> {
        tracing::info!(topic = ?topic, "p2p subscribe stub — Phase 6");
        Ok(())
    }
}

// Phase 6 target — libp2p behaviour stack:
//
// pub struct AxiomBehaviour {
//     kademlia: kad::Behaviour,
//     gossipsub: gossipsub::Behaviour,
//     request_response: request_response::Behaviour,
//     identify: identify::Behaviour,
// }
