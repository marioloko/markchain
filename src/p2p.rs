use crate::block::Block;
use crate::chain::BlockChain;
use crate::error::Result;

use libp2p::{
    core::upgrade,
    floodsub::{Floodsub, FloodsubEvent, Topic},
    futures::StreamExt,
    identity,
    mdns::tokio::Behaviour as Mdns,
    mdns::Event as MdnsEvent,
    mplex,
    noise::{self, NoiseConfig, X25519Spec},
    swarm::{
        ConnectionHandler, IntoConnectionHandler, NetworkBehaviour, Swarm, SwarmBuilder, SwarmEvent,
    },
    tcp::tokio::Transport as TokioTransport,
    PeerId, Transport,
};

use log::{debug, error, info};
use once_cell::sync::Lazy;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

static KEYS: Lazy<identity::Keypair> = Lazy::new(identity::Keypair::generate_ed25519);
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("markchain"));

type MarkChainSwarmEvent = SwarmEvent<<MarkChainBehaviour as NetworkBehaviour>::OutEvent, <<<MarkChainBehaviour as NetworkBehaviour>::ConnectionHandler as IntoConnectionHandler>::Handler as ConnectionHandler>::Error>;

pub struct MarkChainNetwork {
    swarm: Swarm<MarkChainBehaviour>,
    blockchain: BlockChain,
    peers: HashSet<PeerId>,
}

impl MarkChainNetwork {
    pub fn new() -> Result<MarkChainNetwork> {
        info!("Peer ID: {}", PEER_ID.to_base58());

        let auth_keys = noise::Keypair::<X25519Spec>::new().into_authentic(&KEYS)?;

        let transport = TokioTransport::new(Default::default())
            .upgrade(upgrade::Version::V1)
            .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
            .multiplex(mplex::MplexConfig::new())
            .boxed();

        let mut behaviour = MarkChainBehaviour {
            floodsub: Floodsub::new(*PEER_ID),
            mdns: Mdns::new(Default::default())?,
        };

        behaviour.floodsub.subscribe(TOPIC.clone());

        let swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, *PEER_ID).build();

        let blockchain = BlockChain::new();

        let peers = HashSet::new();

        Ok(MarkChainNetwork {
            swarm,
            blockchain,
            peers,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        self.swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

        let mut message_timeout = tokio::time::interval(Duration::from_secs(10));

        loop {
            let event = self.swarm.select_next_some().await;
            self.handle_swarm_event(event);
        }
    }

    fn handle_swarm_event(&mut self, event: MarkChainSwarmEvent) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {address:?}");
            }
            SwarmEvent::Behaviour(markchain_event) => self.handle_markchain_event(markchain_event),
            event => {
                debug!("Unhandled event: {event:?}");
            }
        }
    }

    fn handle_markchain_event(&mut self, event: MarkChainEvent) {
        match event {
            MarkChainEvent::Floodsub(floodsub_event) => self.handle_floodsub_event(floodsub_event),
            MarkChainEvent::Mdns(mdns_event) => self.handle_mdns_event(mdns_event),
            event => {
                debug!("Unhandled markchain event: {event:?}");
            }
        }
    }

    fn handle_floodsub_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(message) => {
                match serde_json::from_slice::<Message>(&message.data) {
                    Ok(message) => info!("{message:#?}"),
                    Err(error) => error!("Deserialization Error: {error}"),
                }
            }
            event => {
                debug!("Unhandled floodsub event: {event:?}");
            }
        }
    }

    fn handle_mdns_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, _) in list {
                    self.peers.insert(peer);
                    self.swarm
                        .behaviour_mut()
                        .floodsub
                        .add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, _) in list {
                    if !self.swarm.behaviour().mdns.has_node(&peer) {
                        self.peers.remove(&peer);
                        self.swarm
                            .behaviour_mut()
                            .floodsub
                            .remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum MessageBody {
    RequestBlock(usize),
    ReturnBlock(Block),
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    to: String,
    from: String,
    content: MessageBody,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MarkChainEvent")]
struct MarkChainBehaviour {
    floodsub: Floodsub,
    mdns: Mdns,
}

#[derive(Debug)]
enum MarkChainEvent {
    Floodsub(FloodsubEvent),
    Mdns(MdnsEvent),
}

impl From<FloodsubEvent> for MarkChainEvent {
    fn from(event: FloodsubEvent) -> MarkChainEvent {
        MarkChainEvent::Floodsub(event)
    }
}

impl From<MdnsEvent> for MarkChainEvent {
    fn from(event: MdnsEvent) -> MarkChainEvent {
        MarkChainEvent::Mdns(event)
    }
}
