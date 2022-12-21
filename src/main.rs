use libp2p::{
    core::upgrade,
    floodsub::{Floodsub, FloodsubEvent, Topic},
    futures::StreamExt,
    identity,
    mdns::tokio::Behaviour as Mdns,
    mdns::Event as MdnsEvent,
    mplex,
    noise::{self, NoiseConfig, X25519Spec},
    swarm::{NetworkBehaviour, Swarm, SwarmBuilder, SwarmEvent},
    tcp::tokio::Transport as TokioTransport,
    PeerId, Transport,
};
use log::{debug, error, info};
use once_cell::sync::Lazy;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashSet;
use std::pin::Pin;
use std::time::Duration;
use tokio::time::Sleep;

static KEYS: Lazy<identity::Keypair> = Lazy::new(identity::Keypair::generate_ed25519);
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("markchain"));

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    to: String,
    from: String,
    kind: String,
    data: Option<Vec<u8>>,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MarkchainEvent")]
struct MarkchainBehaviour {
    floodsub: Floodsub,
    mdns: Mdns,
}

#[derive(Debug)]
enum MarkchainEvent {
    Floodsub(FloodsubEvent),
    Mdns(MdnsEvent),
}

impl From<FloodsubEvent> for MarkchainEvent {
    fn from(event: FloodsubEvent) -> MarkchainEvent {
        MarkchainEvent::Floodsub(event)
    }
}

impl From<MdnsEvent> for MarkchainEvent {
    fn from(event: MdnsEvent) -> MarkchainEvent {
        MarkchainEvent::Mdns(event)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    info!("Peer ID: {}", PEER_ID.to_base58());

    let auth_keys = noise::Keypair::<X25519Spec>::new().into_authentic(&KEYS)?;

    let transport = TokioTransport::new(Default::default())
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    let mut behaviour = MarkchainBehaviour {
        floodsub: Floodsub::new(*PEER_ID),
        mdns: Mdns::new(Default::default())?,
    };

    behaviour.floodsub.subscribe(TOPIC.clone());

    let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, *PEER_ID).build();

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let mut message_timeout = reset_message_timeout();

    let mut peers = HashSet::new();

    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                 match event {
                      SwarmEvent::NewListenAddr { address, .. } => {
                          info!("Listening on {address:?}");
                      }
                      SwarmEvent::Behaviour(markchain_event) => handle_markchain_event(&mut swarm, &mut peers, markchain_event),
                      event => { debug!("Unhandled event: {event:?}"); }
                 }
            }
            () = &mut message_timeout => {
                for peer in peers.iter() {
                    let message = Message { from: PEER_ID.to_base58(), to: peer.to_base58(), kind: "hello".into(), data: None };
                    let json = serde_json::to_vec(&message)?;
                    swarm.behaviour_mut().floodsub.publish(TOPIC.clone(), json);
                }
                message_timeout = reset_message_timeout();
            }
        }
    }
}

fn handle_markchain_event(
    swarm: &mut Swarm<MarkchainBehaviour>,
    peers: &mut HashSet<PeerId>,
    markchain_event: MarkchainEvent,
) {
    match markchain_event {
        MarkchainEvent::Floodsub(FloodsubEvent::Message(message)) => {
            match serde_json::from_slice::<Message>(&message.data) {
                Ok(message) => info!("{message:#?}"),
                Err(error) => error!("Deserialization Error: {error}"),
            }
        }
        MarkchainEvent::Mdns(event) => match event {
            MdnsEvent::Discovered(list) => {
                for (peer, _) in list {
                    peers.insert(peer);
                    swarm
                        .behaviour_mut()
                        .floodsub
                        .add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, _) in list {
                    if !swarm.behaviour().mdns.has_node(&peer) {
                        peers.remove(&peer);
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .remove_node_from_partial_view(&peer);
                    }
                }
            }
        },
        event => {
            debug!("Unhandled markchain event: {event:?}");
        }
    }
}

fn reset_message_timeout() -> Pin<Box<Sleep>> {
    Box::pin(tokio::time::sleep(Duration::from_secs(10)))
}
