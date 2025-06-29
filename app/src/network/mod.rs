pub(crate) mod error;
use error::Error;

pub mod rpc;

use crate::block::{AuxPowHeader, SignedConsensusBlock};
use crate::network::rpc::RPC;
use crate::signatures::IndividualApproval;
use bitcoin::Txid;
use bridge::SingleMemberTransactionSignatures;
use futures::stream::StreamExt;
use libp2p::gossipsub::PublishError;
use libp2p::swarm::{ConnectionId, DialError};
use libp2p::{gossipsub, mdns, noise, swarm::NetworkBehaviour, swarm::SwarmEvent, tcp, yamux};
use libp2p::{Multiaddr, PeerId, Swarm};
use lighthouse_wrapper::types::{BitVector, EthSpec, Hash256, MainnetEthSpec};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::time::{Duration, Instant};
use strum::AsRefStr;
use tokio::io;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::{mpsc, oneshot};
use tracing::*;

pub(crate) use self::rpc::OutboundRequest;
use self::rpc::{
    HandlerErr, NetworkParams, RPCCodedResponse, RPCMessage, RPCReceived, RPCResponse, SubstreamId,
};

pub type EnrAttestationBitfield<T> = BitVector<<T as EthSpec>::SubnetBitfieldLength>;
pub type EnrSyncCommitteeBitfield<T> = BitVector<<T as EthSpec>::SyncCommitteeSubnetCount>;

const RECONNECT_INTERVAL_SECS: u64 = 5;
const RECONNECT_MAX_ATTEMPTS: u32 = 12;

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    /// The Eth2 RPC specified in the wire-0 protocol.
    eth2_rpc: RPC<RequestId, MainnetEthSpec>,
    mdns: mdns::tokio::Behaviour,
}

pub type RequestId = u32;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, AsRefStr)]
#[strum(serialize_all = "snake_case")]
/// Used for libp2p's `topic` field
pub enum GossipKind {
    ConsensusBlock,
    ApproveBlock,
    QueuePow,
    PegoutSignatures,
}
impl GossipKind {
    fn topic(&self) -> gossipsub::IdentTopic {
        gossipsub::IdentTopic::new(self.as_ref())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveBlock {
    pub block_hash: Hash256,
    pub signature: IndividualApproval,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PubsubMessage {
    ConsensusBlock(SignedConsensusBlock<MainnetEthSpec>),
    ApproveBlock(ApproveBlock),
    QueuePow(AuxPowHeader),
    PegoutSignatures(HashMap<Txid, SingleMemberTransactionSignatures>),
}

impl PubsubMessage {
    fn topic(&self) -> gossipsub::IdentTopic {
        self.kind().topic()
    }

    fn kind(&self) -> GossipKind {
        match self {
            Self::ConsensusBlock(_) => GossipKind::ConsensusBlock,
            Self::ApproveBlock(_) => GossipKind::ApproveBlock,
            Self::QueuePow(_) => GossipKind::QueuePow,
            Self::PegoutSignatures(_) => GossipKind::PegoutSignatures,
        }
    }
}

#[allow(clippy::large_enum_variant)]
enum FrontToBackCommand {
    Publish(PubsubMessage, oneshot::Sender<Result<(), PublishError>>),
    SendRpc(
        PeerId,
        OutboundRequest<MainnetEthSpec>,
        oneshot::Sender<mpsc::Receiver<RPCResponse<MainnetEthSpec>>>,
    ),
    RespondRpc(
        PeerId,
        ConnectionId,
        SubstreamId,
        RPCCodedResponse<MainnetEthSpec>,
        oneshot::Sender<Result<(), PublishError>>,
    ),
    Dial(Multiaddr, oneshot::Sender<Result<(), DialError>>),
    SubscribeEvents(oneshot::Sender<broadcast::Receiver<PubsubMessage>>),
    SubscribeRpcEvents(oneshot::Sender<broadcast::Receiver<RPCMessage<RequestId, MainnetEthSpec>>>),
    SubscribePeers(oneshot::Sender<broadcast::Receiver<HashSet<PeerId>>>),
}

#[derive(Clone)]
pub struct Client {
    front_to_back_tx: mpsc::Sender<FrontToBackCommand>,
}

impl Client {
    pub async fn publish_block(
        &self,
        block: SignedConsensusBlock<MainnetEthSpec>,
    ) -> Result<(), Error> {
        self.send(PubsubMessage::ConsensusBlock(block)).await
    }

    pub async fn send(&self, message: PubsubMessage) -> Result<(), Error> {
        let (sender, receiver) = oneshot::channel();

        self.front_to_back_tx
            .send(FrontToBackCommand::Publish(message, sender))
            .await
            .map_err(|_| Error::ChannelSendError)?;
        receiver.await?.map_err(Into::into)
    }

    pub async fn send_rpc(
        &self,
        peer_id: PeerId,
        req: OutboundRequest<MainnetEthSpec>,
    ) -> Result<mpsc::Receiver<RPCResponse<MainnetEthSpec>>, Error> {
        let (sender, receiver) = oneshot::channel();

        self.front_to_back_tx
            .send(FrontToBackCommand::SendRpc(peer_id, req, sender))
            .await
            .map_err(|_| Error::ChannelSendError)?;
        Ok(receiver.await?)
    }

    pub async fn respond_rpc(
        &self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        substream_id: SubstreamId,
        payload: RPCCodedResponse<MainnetEthSpec>,
    ) -> Result<(), Error> {
        let (sender, receiver) = oneshot::channel();

        self.front_to_back_tx
            .send(FrontToBackCommand::RespondRpc(
                peer_id,
                connection_id,
                substream_id,
                payload,
                sender,
            ))
            .await
            .map_err(|_| Error::ChannelSendError)?;
        receiver.await?.map_err(Into::into)
    }

    #[allow(unused)]
    pub async fn dial(&self, address: Multiaddr) -> Result<(), Error> {
        let (sender, receiver) = oneshot::channel();

        self.front_to_back_tx
            .send(FrontToBackCommand::Dial(address, sender))
            .await
            .map_err(|_| Error::ChannelSendError)?;
        receiver.await?.map_err(Into::into)
    }

    pub async fn subscribe_events(&self) -> Result<broadcast::Receiver<PubsubMessage>, Error> {
        let (sender, receiver) = oneshot::channel();

        self.front_to_back_tx
            .send(FrontToBackCommand::SubscribeEvents(sender))
            .await
            .map_err(|_| Error::ChannelSendError)?;
        receiver.await.map_err(Into::into)
    }

    pub async fn subscribe_peers(&self) -> Result<broadcast::Receiver<HashSet<PeerId>>, Error> {
        let (sender, receiver) = oneshot::channel();

        self.front_to_back_tx
            .send(FrontToBackCommand::SubscribePeers(sender))
            .await
            .map_err(|_| Error::ChannelSendError)?;
        receiver.await.map_err(Into::into)
    }

    pub async fn subscribe_rpc_events(
        &self,
    ) -> Result<broadcast::Receiver<RPCMessage<RequestId, MainnetEthSpec>>, Error> {
        let (sender, receiver) = oneshot::channel();

        self.front_to_back_tx
            .send(FrontToBackCommand::SubscribeRpcEvents(sender))
            .await
            .map_err(|_| Error::ChannelSendError)?;
        receiver.await.map_err(Into::into)
    }
}

/// Information about a peer's reconnection attempts
#[derive(Debug, Clone)]
struct PeerReconnectInfo {
    /// The multiaddress of the peer to reconnect to
    address: Multiaddr,
    /// Timestamp of the last reconnection attempt, if any
    last_attempt: Option<Instant>,
    /// Number of reconnection attempts made so far
    attempt_count: u32,
}

impl PeerReconnectInfo {
    fn new(address: Multiaddr) -> Self {
        Self {
            address,
            last_attempt: None,
            attempt_count: 0,
        }
    }

    fn should_reconnect(&self) -> bool {
        match self.last_attempt {
            None => true,
            Some(last) => {
                let backoff_duration = Duration::from_secs(2_u64.pow(self.attempt_count.min(12)));
                last.elapsed() >= backoff_duration
            }
        }
    }

    fn mark_attempt(&mut self) {
        self.last_attempt = Some(Instant::now());
        self.attempt_count += 1;
    }

    fn reset_attempts(&mut self) {
        self.attempt_count = 0;
        self.last_attempt = None;
    }
}

/// Internal network backend that handles all network operations
struct NetworkBackend {
    /// Channel receiver for commands from the client
    front_to_back_rx: mpsc::Receiver<FrontToBackCommand>,
    /// The libp2p swarm managing network connections
    swarm: Swarm<MyBehaviour>,
    /// Mapping of peer IDs to their multiaddresses for reconnection
    peer_addresses: HashMap<PeerId, Multiaddr>,
    /// Information about peers that need reconnection attempts
    reconnect_info: HashMap<PeerId, PeerReconnectInfo>,
}

impl NetworkBackend {
    async fn run(mut self) {
        let (network_event_tx, _rx) = broadcast::channel(32);
        let (network_rpc_event_tx, _rx) = broadcast::channel(64);
        let (peers_connected_tx, _rx) = broadcast::channel(32);

        let mut peers = HashSet::new();

        let mut rpc_response_channels: HashMap<
            RequestId,
            mpsc::Sender<RPCResponse<MainnetEthSpec>>,
        > = HashMap::new();
        let mut next_id = 0;

        let mut reconnect_timer =
            tokio::time::interval(Duration::from_secs(RECONNECT_INTERVAL_SECS));

        loop {
            select! {
                _ = reconnect_timer.tick() => {
                    self.attempt_reconnections().await;
                }
                maybe_message = self.front_to_back_rx.recv() => match maybe_message {
                    Some(FrontToBackCommand::Publish(msg, response)) => {
                        let result = self.swarm
                            .behaviour_mut().gossipsub
                            .publish(msg.topic(), rmp_serde::to_vec(&msg).unwrap())
                            .map(|_| ());

                        // if sending the response fails, there is nothing we can do, so ignore
                        let _ = response.send(result);
                    }
                    Some(FrontToBackCommand::Dial(address, response)) => {
                        info!("Dialing to peer at address: {address}");
                        let result = self.swarm.dial(address);
                        // if sending the response fails, there is nothing we can do, so ignore
                        let _ = response.send(result);
                    }
                    Some(FrontToBackCommand::SubscribeEvents(response)) => {
                        let rx = network_event_tx.subscribe();
                        // if sending the response fails, there is nothing we can do, so ignore
                        let _ = response.send(rx);
                    }
                    Some(FrontToBackCommand::SubscribeRpcEvents(response)) => {
                        let rx = network_rpc_event_tx.subscribe();
                        // if sending the response fails, there is nothing we can do, so ignore
                        let _ = response.send(rx);
                    }
                    Some(FrontToBackCommand::SubscribePeers(response)) => {
                        let rx = peers_connected_tx.subscribe();
                        // if sending the response fails, there is nothing we can do, so ignore
                        let _ = response.send(rx);

                        // send list of peers that were already connected
                        // TODO: handle error?
                        let _ = peers_connected_tx.send(peers.clone());
                    }
                    Some(FrontToBackCommand::SendRpc(peer_id, req, response)) => {
                        info!("Sending rpc...");
                        self.swarm.behaviour_mut().eth2_rpc.send_request(peer_id, next_id, req);

                        let (tx, rx) = mpsc::channel(1024);
                        rpc_response_channels.insert(next_id, tx);
                        response.send(rx).unwrap();
                        next_id += 1;
                    }
                    Some(FrontToBackCommand::RespondRpc(peer_id, connection_id, substream_id, payload, _response)) => {
                        self.swarm.behaviour_mut().eth2_rpc.send_response(peer_id, (connection_id, substream_id), payload);
                    }
                    None => {
                        // channel shut down, nothing to do
                    }
                },
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    })) => {
                        debug!(
                            "Got message: '{}' with id: {id} from peer: {peer_id}",
                            String::from_utf8_lossy(&message.data),
                        );
                        let msg = rmp_serde::from_slice(&message.data).unwrap(); // todo: better handling
                        // if sending the response fails, there is nothing we can do, so ignore
                        let _ = network_event_tx.send(msg);
                    },
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer_id, _multiaddr) in list {
                            debug!("mDNS discovered a new peer: {peer_id}");
                            self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);

                            peers.insert(peer_id);

                            let _ = peers_connected_tx.send(peers.clone());
                        }
                    },
                    SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                        for (peer_id, _multiaddr) in list {
                            debug!("mDNS discover peer has expired: {peer_id}");
                            self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);

                            // also send update of expiry
                            let _ = peers_connected_tx.send(peers.clone());
                        }
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        debug!("Local node is listening on {address}");
                    }
                    SwarmEvent::Behaviour(MyBehaviourEvent::Eth2Rpc(x)) => {
                        match &x.event {
                            Ok(RPCReceived::Request(_substream_id, _request)) => {
                                // send to rpc listener
                                let _ = network_rpc_event_tx.send(x);
                            }
                            Ok(RPCReceived::Response(request_id, received_response)) => {
                                // propagate response
                                // todo: make robust
                                let _res = rpc_response_channels[request_id].send(received_response.clone()).await;
                            }
                            Ok(RPCReceived::EndOfStream(request_id, _)) => {
                                rpc_response_channels.remove(request_id);
                            }
                            Err(HandlerErr::Inbound { id: err_stream_id, proto: _, error: stream_error }) => {
                                // not sure what to do with this, ignore for now
                                warn!("Inbound error: {:?} - Id: {:?}", stream_error, err_stream_id);
                            }
                            Err(HandlerErr::Outbound { id: stream_id, proto: _, error: stream_err }) => {
                                warn!("Outbound error: {:?} - Id: {:?}", stream_err, stream_id);
                            }
                        }

                    }
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                        peers.insert(peer_id);

                        // Store peer address for potential reconnection
                        self.peer_addresses.insert(peer_id, endpoint.get_remote_address().clone());

                        // Reset reconnection attempts on successful connection
                        if let Some(info) = self.reconnect_info.get_mut(&peer_id) {
                            info.reset_attempts();
                        }

                        let _ = peers_connected_tx.send(peers.clone());
                    }
                    SwarmEvent::ConnectionClosed { peer_id, connection_id, endpoint, num_established, cause } => {
                        debug!("Connection closed: peer_id: {peer_id}, connection_id: {connection_id}, endpoint: {endpoint:?}, num_established: {num_established}, cause: {cause:?}");

                        // Only remove from peers if no more connections to this peer
                        if num_established == 0 {
                            peers.remove(&peer_id);

                            // Set up for reconnection if we have the address and it was an unexpected disconnection
                            if let Some(address) = self.peer_addresses.get(&peer_id) {
                                if !matches!(cause, Some(libp2p::swarm::ConnectionError::KeepAliveTimeout)) {
                                    // Only reconnect for unexpected disconnections (not timeouts)
                                    debug!("Scheduling reconnection attempt for peer {peer_id}");
                                    self.reconnect_info.insert(peer_id, PeerReconnectInfo::new(address.clone()));
                                }
                            }

                            let _ = peers_connected_tx.send(peers.clone());
                        }
                    }
                    x => {
                        trace!("Unhandled message {x:?}");
                    }
                }
            }
        }
    }

    async fn attempt_reconnections(&mut self) {
        let mut to_reconnect = Vec::new();

        // Collect peers that should be reconnected
        for (peer_id, info) in &mut self.reconnect_info {
            if info.should_reconnect() {
                to_reconnect.push((*peer_id, info.address.clone()));
                info.mark_attempt();
            }
        }

        // Attempt reconnections
        for (peer_id, address) in to_reconnect {
            info!("Attempting to reconnect to peer {peer_id} at {address}");
            match self.swarm.dial(address) {
                Ok(_) => {
                    info!("Reconnection dial initiated for peer {peer_id}");
                }
                Err(e) => {
                    warn!("Failed to initiate reconnection to peer {peer_id}: {e}");
                    // If we can't dial after many attempts, remove from reconnect list
                    if let Some(info) = self.reconnect_info.get(&peer_id) {
                        if info.attempt_count > RECONNECT_MAX_ATTEMPTS {
                            warn!(
                                "Giving up reconnection attempts for peer {peer_id} after {RECONNECT_MAX_ATTEMPTS} tries"
                            );
                            self.reconnect_info.remove(&peer_id);
                            self.peer_addresses.remove(&peer_id);
                        }
                    }
                }
            }
        }
    }
}

pub async fn spawn_network_handler(
    addr: String,
    port: u16,
    remote_bootnode: Option<String>,
) -> Result<Client, Error> {
    let (tx, rx) = mpsc::channel(32);
    let client = Client {
        front_to_back_tx: tx,
    };

    let mut swarm = create_swarm()?;

    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&GossipKind::ApproveBlock.topic())?;
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&GossipKind::ConsensusBlock.topic())?;
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&GossipKind::QueuePow.topic())?;
    swarm
        .behaviour_mut()
        .gossipsub
        .subscribe(&GossipKind::PegoutSignatures.topic())?;

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on(format!("/ip4/{addr}/udp/{port}/quic-v1").parse()?)?;
    swarm.listen_on(format!("/ip4/{addr}/tcp/{port}").parse()?)?;
    let backend = NetworkBackend {
        front_to_back_rx: rx,
        swarm,
        peer_addresses: HashMap::new(),
        reconnect_info: HashMap::new(),
    };

    tokio::spawn(async move {
        backend.run().await;
    });

    if let Some(bootnode) = remote_bootnode {
        trace!("Dialing bootnode: {}", bootnode);
        let address = Multiaddr::from_str(&bootnode)?;
        client.dial(address).await?;
    }

    Ok(client)
}

fn create_swarm() -> Result<Swarm<MyBehaviour>, Error> {
    let swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| {
            // To content-address message, we can take the hash of message and use it as an ID.
            let message_id_fn = |message: &gossipsub::Message| {
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_string())
            };

            // Set a custom gossipsub configuration
            #[allow(clippy::io_other_error)]
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
                .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
                .build()
                .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?; // Temporary hack because `build` does not return a proper `std::error::Error`.

            // build a gossipsub network behaviour
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;

            let network_params = NetworkParams {
                max_chunk_size: 1024 * 1024_usize,
                ttfb_timeout: Duration::from_secs(180),
                resp_timeout: Duration::from_secs(180),
            };

            let drain = slog::Discard;

            let root_logger = slog::Logger::root(drain, slog::o!());

            let eth2_rpc = RPC::new(
                Default::default(),
                Default::default(),
                root_logger,
                network_params,
            );

            Ok(MyBehaviour {
                gossipsub,
                eth2_rpc,
                mdns,
            })
        })
        .map_err(|_| Error::BehaviorError)?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(180)))
        .build();
    Ok(swarm)
}
