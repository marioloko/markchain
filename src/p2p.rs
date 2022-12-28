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

use log::{debug, info, warn};
use serde_derive::{Deserialize, Serialize};
use std::time::Duration;

static TOPIC: &str = "markchain";

type MarkChainSwarmEvent = SwarmEvent<<MarkChainBehaviour as NetworkBehaviour>::OutEvent, <<<MarkChainBehaviour as NetworkBehaviour>::ConnectionHandler as IntoConnectionHandler>::Handler as ConnectionHandler>::Error>;

pub struct MarkChainNetwork {
    swarm: Swarm<MarkChainBehaviour>,
    blockchain: BlockChain,
    peer_id: PeerId,
    topic: Topic,
}

impl MarkChainNetwork {
    pub fn new(keys: identity::Keypair) -> Result<MarkChainNetwork> {
        let peer_id = PeerId::from(keys.public());
        info!("Peer ID: {}", peer_id.to_base58());

        let auth_keys = noise::Keypair::<X25519Spec>::new().into_authentic(&keys)?;

        let transport = TokioTransport::new(Default::default())
            .upgrade(upgrade::Version::V1)
            .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
            .multiplex(mplex::MplexConfig::new())
            .boxed();

        let mut behaviour = MarkChainBehaviour {
            floodsub: Floodsub::new(peer_id),
            mdns: Mdns::new(Default::default())?,
        };

        let topic = Topic::new(TOPIC);
        behaviour.floodsub.subscribe(topic.clone());

        let swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build();

        let blockchain = BlockChain::new();

        Ok(MarkChainNetwork {
            swarm,
            blockchain,
            peer_id,
            topic,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        self.swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

        let init_request_timeout = tokio::time::sleep(Duration::from_secs(10));
        tokio::pin!(init_request_timeout);

        // At the beggining the timeout and swarm events can arrive in any order, so
        // use select to choose the futures which finishes before.
        loop {
            tokio::select! {
                () = &mut init_request_timeout => {
                    self.request_block_at(1)?;
                    break;
                }
                event = self.swarm.select_next_some() => self.handle_swarm_event(event)?,
            }
        }

        // After the timeout we only need to wait for swarm events, so there is no need
        // to use select.
        loop {
            let event = self.swarm.select_next_some().await;
            self.handle_swarm_event(event)?;
        }
    }

    fn request_block_at(&mut self, index: usize) -> Result<()> {
        let to = Receiver::All;
        let content = MessageBody::RequestBlock(index);
        self.send_message(to, content)
    }

    fn respond_with_block(&mut self, peer_id: PeerId, block: Block) -> Result<()> {
        let to = Receiver::One(peer_id);
        let content = MessageBody::ReturnBlock(block);
        self.send_message(to, content)
    }

    fn send_message(&mut self, to: Receiver, content: MessageBody) -> Result<()> {
        let message = Message {
            from: self.peer_id,
            to,
            content,
        };
        let json = serde_json::to_vec(&message)?;
        let topic = self.topic.clone();
        self.swarm.behaviour_mut().floodsub.publish(topic, json);
        Ok(())
    }

    fn handle_swarm_event(&mut self, event: MarkChainSwarmEvent) -> Result<()> {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {address:?}");
            }
            SwarmEvent::Behaviour(markchain_event) => {
                self.handle_markchain_event(markchain_event)?
            }
            event => {
                debug!("Unhandled event: {event:?}");
            }
        }
        Ok(())
    }

    fn handle_markchain_event(&mut self, event: MarkChainEvent) -> Result<()> {
        match event {
            MarkChainEvent::Floodsub(floodsub_event) => {
                self.handle_floodsub_event(floodsub_event)?
            }
            MarkChainEvent::Mdns(mdns_event) => self.handle_mdns_event(mdns_event),
        }
        Ok(())
    }

    fn handle_floodsub_event(&mut self, event: FloodsubEvent) -> Result<()> {
        match event {
            FloodsubEvent::Message(message) => {
                let message = serde_json::from_slice::<Message>(&message.data)?;
                match message.to {
                    Receiver::All => self.handle_message(message)?,
                    Receiver::One(peer_id) if peer_id == self.peer_id => {
                        self.handle_message(message)?
                    }
                    _ => {}
                }
            }
            event => {
                debug!("Unhandled floodsub event: {event:?}");
            }
        }
        Ok(())
    }

    fn handle_message(&mut self, message: Message) -> Result<()> {
        info!("{message:?}");
        match message.content {
            MessageBody::RequestBlock(index) => match self.blockchain.get_block(index) {
                None => warn!("No block found at index: {index}"),
                Some(block) => self.respond_with_block(message.from, block.clone())?,
            },
            MessageBody::ReturnBlock(block) => {
                self.blockchain.add_block(block);
                let next_block_index = self.blockchain.len();
                info!("request next block at index: {next_block_index}");
                self.request_block_at(next_block_index)?;
            }
        }
        Ok(())
    }

    fn handle_mdns_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, _) in list {
                    self.swarm
                        .behaviour_mut()
                        .floodsub
                        .add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, _) in list {
                    if !self.swarm.behaviour().mdns.has_node(&peer) {
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Receiver {
    All,
    One(PeerId),
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    to: Receiver,
    from: PeerId,
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
