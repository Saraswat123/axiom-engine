use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use futures::StreamExt;
use libp2p::{
    gossipsub, identify, kad, noise, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// ─── Message types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerMessage {
    pub from: String,
    pub topic: MessageTopic,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MessageTopic {
    ProofArtifact,
    ProveRequest,
    TraceGossip,
    InvariantUpdate,
}

impl MessageTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageTopic::ProofArtifact => "axiom/proof/v1",
            MessageTopic::ProveRequest => "axiom/prove-req/v1",
            MessageTopic::TraceGossip => "axiom/trace/v1",
            MessageTopic::InvariantUpdate => "axiom/invariant/v1",
        }
    }
}

// ─── Commands sent to swarm task ─────────────────────────────────────────────

enum P2pCommand {
    Publish { topic: MessageTopic, data: Vec<u8> },
    Dial(Multiaddr),
}

// ─── Public handle (cloneable) ────────────────────────────────────────────────

#[derive(Clone)]
pub struct P2pHandle {
    cmd_tx: mpsc::Sender<P2pCommand>,
    peer_count: Arc<AtomicUsize>,
    local_peer_id: PeerId,
}

impl P2pHandle {
    pub async fn publish(&self, topic: MessageTopic, data: Vec<u8>) -> Result<()> {
        self.cmd_tx.send(P2pCommand::Publish { topic, data }).await?;
        Ok(())
    }

    pub async fn dial(&self, addr: Multiaddr) -> Result<()> {
        self.cmd_tx.send(P2pCommand::Dial(addr)).await?;
        Ok(())
    }

    pub fn peer_count(&self) -> usize {
        self.peer_count.load(Ordering::Relaxed)
    }

    pub fn local_peer_id(&self) -> &PeerId {
        &self.local_peer_id
    }
}

// ─── Combined network behaviour ───────────────────────────────────────────────

#[derive(NetworkBehaviour)]
struct AxiomBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

// ─── Node builder ─────────────────────────────────────────────────────────────

pub struct P2pNode {
    listen_addr: Multiaddr,
    bootstrap_peers: Vec<Multiaddr>,
}

impl P2pNode {
    pub fn new(listen_addr: Multiaddr, bootstrap_peers: Vec<Multiaddr>) -> Self {
        Self { listen_addr, bootstrap_peers }
    }

    pub fn from_env() -> Self {
        let port = std::env::var("AXIOM_P2P_PORT").unwrap_or_else(|_| "9000".to_string());
        let listen_addr = format!("/ip4/0.0.0.0/tcp/{port}")
            .parse()
            .expect("valid listen addr");

        let bootstrap_peers = std::env::var("AXIOM_PEERS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        Self { listen_addr, bootstrap_peers }
    }

    pub fn start(self) -> Result<P2pHandle> {
        let peer_count = Arc::new(AtomicUsize::new(0));
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<P2pCommand>(64);

        let mut swarm = build_swarm()?;
        let local_peer_id = *swarm.local_peer_id();

        // Subscribe to all topics
        for topic in &[
            MessageTopic::ProofArtifact,
            MessageTopic::ProveRequest,
            MessageTopic::TraceGossip,
            MessageTopic::InvariantUpdate,
        ] {
            let gs_topic = gossipsub::IdentTopic::new(topic.as_str());
            swarm.behaviour_mut().gossipsub.subscribe(&gs_topic)?;
        }

        // Start listening
        swarm.listen_on(self.listen_addr.clone())?;

        // Dial bootstrap peers
        for addr in &self.bootstrap_peers {
            if let Err(e) = swarm.dial(addr.clone()) {
                tracing::warn!(%addr, "bootstrap dial failed: {e}");
            }
        }

        let peer_count_clone = peer_count.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = swarm.select_next_some() => {
                        handle_event(&mut swarm, event, &peer_count_clone);
                    }
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(P2pCommand::Publish { topic, data }) => {
                                let gs_topic = gossipsub::IdentTopic::new(topic.as_str());
                                if let Err(e) = swarm.behaviour_mut().gossipsub.publish(gs_topic, data) {
                                    tracing::warn!("gossipsub publish failed: {e}");
                                }
                            }
                            Some(P2pCommand::Dial(addr)) => {
                                if let Err(e) = swarm.dial(addr.clone()) {
                                    tracing::warn!(%addr, "dial failed: {e}");
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        tracing::info!(
            peer_id = %local_peer_id,
            listen = %self.listen_addr,
            "p2p swarm started"
        );

        Ok(P2pHandle { cmd_tx, peer_count, local_peer_id })
    }
}

// ─── Swarm event handler ──────────────────────────────────────────────────────

fn handle_event(
    swarm: &mut Swarm<AxiomBehaviour>,
    event: SwarmEvent<AxiomBehaviourEvent>,
    peer_count: &Arc<AtomicUsize>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            tracing::info!(%address, "p2p listening");
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            peer_count.fetch_add(1, Ordering::Relaxed);
            tracing::info!(%peer_id, peers = peer_count.load(Ordering::Relaxed), "peer connected");
            // Add to kademlia routing table
            swarm.behaviour_mut().kademlia.add_address(&peer_id, "/ip4/0.0.0.0/tcp/0".parse().unwrap());
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            let prev = peer_count.load(Ordering::Relaxed);
            if prev > 0 { peer_count.fetch_sub(1, Ordering::Relaxed); }
            tracing::info!(%peer_id, "peer disconnected");
        }
        SwarmEvent::Behaviour(AxiomBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            propagation_source,
            message,
            ..
        })) => {
            let topic = message.topic.as_str().to_string();
            tracing::info!(
                from = %propagation_source,
                topic,
                bytes = message.data.len(),
                "gossipsub message received"
            );
        }
        SwarmEvent::Behaviour(AxiomBehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
            ..
        })) => {
            tracing::debug!(%peer_id, agent = %info.agent_version, "peer identified");
            for addr in info.listen_addrs {
                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
            }
        }
        SwarmEvent::Behaviour(AxiomBehaviourEvent::Kademlia(
            kad::Event::RoutingUpdated { peer, .. },
        )) => {
            tracing::debug!(%peer, "kademlia routing updated");
        }
        _ => {}
    }
}

// ─── Swarm factory ────────────────────────────────────────────────────────────

fn build_swarm() -> Result<Swarm<AxiomBehaviour>> {
    let swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            let local_peer_id = PeerId::from(key.public());

            // Gossipsub — message deduplication via content hash
            let msg_id_fn = |msg: &gossipsub::Message| {
                let mut hasher = DefaultHasher::new();
                msg.data.hash(&mut hasher);
                gossipsub::MessageId::from(hasher.finish().to_string())
            };
            let gossipsub_cfg = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .message_id_fn(msg_id_fn)
                .build()
                .map_err(|e| anyhow::anyhow!("gossipsub config: {e}"))?;

            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_cfg,
            )
            .map_err(|e| anyhow::anyhow!("gossipsub: {e}"))?;

            // Kademlia — DHT peer discovery
            let kad_protocol: libp2p::StreamProtocol =
                libp2p::StreamProtocol::try_from_owned("/axiom-engine/kad/1.0.0".to_string())
                    .map_err(|e| anyhow::anyhow!("kad protocol: {e}"))?;
            let mut kademlia_cfg = kad::Config::new(kad_protocol);
            kademlia_cfg.set_query_timeout(Duration::from_secs(60));
            let kademlia = kad::Behaviour::with_config(
                local_peer_id,
                kad::store::MemoryStore::new(local_peer_id),
                kademlia_cfg,
            );

            // Identify — exchange node info on connect
            let identify = identify::Behaviour::new(identify::Config::new(
                "/axiom-engine/identify/1.0.0".to_string(),
                key.public(),
            ));

            // Ping — keepalive
            let ping = ping::Behaviour::new(ping::Config::new());

            Ok(AxiomBehaviour { gossipsub, kademlia, identify, ping })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    Ok(swarm)
}
