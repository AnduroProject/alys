use thiserror::Error;

use libp2p::gossipsub::{PublishError, SubscriptionError};
use libp2p::noise::Error as Libp2pNoiseError;
use libp2p::swarm::DialError;
use libp2p::TransportError;
pub(crate) use tokio::sync::oneshot::error::RecvError as OneshotRecvError;

#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to send message over channel")]
    ChannelSendError,
    #[error("Failed to read from one-shot channel")]
    OneshotRecvError(#[from] OneshotRecvError),
    #[error("Noise error")]
    Libp2pNoiseError(#[from] Libp2pNoiseError),
    #[error("Libp2p subscription error")]
    Libp2pSubscriptionError(#[from] SubscriptionError),
    #[error("Failed to build behavior")]
    BehaviorError, // actual error is not exposed: https://github.com/libp2p/rust-libp2p/issues/4829
    #[error("multiaddr error")]
    MultiaddrError(#[from] libp2p::multiaddr::Error),
    #[error("Libp2p TransportError error")]
    Libp2pTransportError(#[from] TransportError<std::io::Error>),
    #[error("Libp2p dial error")]
    Libp2pDialError(#[from] DialError),
    #[error("Libp2p publish error")]
    Libp2pPublishError(#[from] PublishError),
}
