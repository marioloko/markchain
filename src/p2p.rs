use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use libp2p::{
    core::upgrade,
    floodsub::{Floodsub, FloodsubEvent, Topic},
    futures::{stream, Stream, StreamExt},
    identity,
    mdns::tokio::Behaviour as Mdns,
    mdns::Event as MdnsEvent,
    mplex,
    noise::{self, NoiseConfig, X25519Spec},
    swarm::{
        keep_alive, ConnectionHandler, IntoConnectionHandler, NetworkBehaviour, Swarm,
        SwarmBuilder, SwarmEvent,
    },
    tcp::tokio::Transport as TokioTransport,
    PeerId, Transport,
};
use log::{debug, error, info, warn};
use rand::seq::IteratorRandom;
use serde_derive::{Deserialize, Serialize};
use tokio::sync::mpsc as tokio_mpsc;

use crate::blockchain::{Block, BlockChain};
use crate::db::Db;
use crate::error::Result;
use crate::wallet::WalletManager;

const CHANNEL_SIZE: usize = 2_048;
const TOPIC: &str = "markchain";

/// The minimum number of validators to vote for a validator. If less than
/// this validator no new block will be created.
const MIN_VALIDATORS: usize = 3;

/// The percentage of validators which should have vote for an election to be valid.
const MIN_VOTE_PERCENTAGE: f64 = 0.75;

type MarkChainSwarmEvent = SwarmEvent<<MarkChainBehaviour as NetworkBehaviour>::OutEvent, <<<MarkChainBehaviour as NetworkBehaviour>::ConnectionHandler as IntoConnectionHandler>::Handler as ConnectionHandler>::Error>;

type Votes = HashMap<usize, HashMap<PeerId, PeerId>>;

pub struct MarkChainNetwork {
    swarm: Swarm<MarkChainBehaviour>,
    blockchain: BlockChain,
    peer_id: PeerId,
    topic: Topic,
    validators: HashSet<PeerId>,
    votes: Votes,
    wallet_manager: WalletManager,
    event_sender: tokio_mpsc::Sender<TriggerEvent>,
    event_receiver: tokio_mpsc::Receiver<TriggerEvent>,
}

impl MarkChainNetwork {
    pub fn try_new<P: AsRef<Path>>(
        keys: identity::Keypair,
        db_path: &P,
    ) -> Result<MarkChainNetwork> {
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
            keep_alive: keep_alive::Behaviour::default(),
        };

        let topic = Topic::new(TOPIC);
        behaviour.floodsub.subscribe(topic.clone());

        let swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build();

        let storage = Db::open(db_path)?;

        let blockchain = BlockChain::try_new(&storage)?;

        let wallet_manager = WalletManager::try_new(&storage)?;

        let (event_sender, event_receiver) = tokio_mpsc::channel(CHANNEL_SIZE);

        let mut validators = HashSet::new();
        validators.insert(peer_id);

        Ok(MarkChainNetwork {
            swarm,
            blockchain,
            peer_id,
            topic,
            validators,
            votes: Votes::new(),
            wallet_manager,
            event_sender,
            event_receiver,
        })
    }

    pub fn into_event_stream(mut self) -> Result<impl Stream<Item = Result<()>>> {
        self.swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

        let default_wallet = self.wallet_manager.default_wallet()?;
        info!("Wallet address: {:?}", default_wallet.address());

        // Synchronize the first block.
        let event_sender = self.event_sender.clone();
        let next_index = self.blockchain.len();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Err(err) = event_sender
                .send(TriggerEvent::RequestBlock(next_index))
                .await
            {
                error!("error synchronizing first block: {err}");
            }
        });

        // Requesting the validators.
        let event_sender = self.event_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Err(err) = event_sender.send(TriggerEvent::RequestAllValidators).await {
                error!("error requesting all validators: {err}");
            }
        });

        // Sync the validators.
        let event_sender = self.event_sender.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(10)).await;
            if let Err(err) = event_sender.send(TriggerEvent::RegisterValidator).await {
                error!("error synchronizing validators: {err}");
            }
        });

        // Vote periodically
        let event_sender = self.event_sender.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(20));
            interval.tick().await;

            loop {
                interval.tick().await;
                if let Err(err) = event_sender.send(TriggerEvent::Vote).await {
                    error!("error voting: {err}");
                }
            }
        });

        // Create a block periodically
        let event_sender = self.event_sender.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            interval.tick().await;

            loop {
                interval.tick().await;
                if let Err(err) = event_sender.send(TriggerEvent::CreateBlock).await {
                    error!("error creating block: {err}");
                }
            }
        });

        let stream = stream::unfold(self, |mut network| async move {
            let result = network.handle_next_event().await;
            Some((result, network))
        });

        Ok(stream)
    }

    async fn handle_next_event(&mut self) -> Result<()> {
        tokio::select! {
            Some(event) = self.event_receiver.recv() => self.handle_async_network_event(event)?,
            event = self.swarm.select_next_some() => self.handle_swarm_event(event)?,
        }
        Ok(())
    }

    fn request_block_at(&mut self, index: usize) -> Result<()> {
        let to = Receiver::All;
        let content = MessageBody::RequestBlock(index);
        self.send_message(to, content)
    }

    fn sync_block(&mut self, peer_id: PeerId, block: Block) -> Result<()> {
        let to = Receiver::One(peer_id);
        let content = MessageBody::SyncBlock(block);
        self.send_message(to, content)
    }

    fn request_validators(&mut self) -> Result<()> {
        let to = Receiver::All;
        let content = MessageBody::RequestAllValidators;
        self.send_message(to, content)
    }

    fn register_validator(&mut self, validator: PeerId) -> Result<()> {
        let to = Receiver::All;
        let content = MessageBody::RegisterValidator(validator);
        self.send_message(to, content)
    }

    fn vote(&mut self) -> Result<()> {
        let mut rng = rand::thread_rng();
        let index = self.blockchain.len();
        let vote_to = self
            .validators
            .iter()
            .choose(&mut rng)
            .expect("Expected at least one validator");
        let vote = Vote {
            index,
            to: *vote_to,
        };

        self.insert_vote(self.peer_id, vote);

        let to = Receiver::All;
        let content = MessageBody::Vote(vote);
        self.send_message(to, content)
    }

    fn create_block(&mut self) -> Result<()> {
        let block = self.blockchain.generate_next_block(None)?;
        if self.blockchain.add_block(block.clone())? {
            let to = Receiver::All;
            let content = MessageBody::NewBlock(block);
            self.send_message(to, content)?;
        }
        Ok(())
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

    fn handle_async_network_event(&mut self, event: TriggerEvent) -> Result<()> {
        match event {
            TriggerEvent::RequestBlock(index) => self.request_block_at(index)?,
            TriggerEvent::RequestAllValidators => self.request_validators()?,
            TriggerEvent::RegisterValidator => {
                self.register_validator(self.peer_id)?;
            }
            TriggerEvent::Vote => self.vote()?,
            TriggerEvent::CreateBlock => {
                self.remove_deprecated_votes();
                if self.has_quorum() {
                    if let Some(peer_id) = self.choose_validator() {
                        info!("Chosen validator: {peer_id}");
                        if self.peer_id == peer_id {
                            self.create_block()?;
                        }
                    }
                }
            }
        }
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
            _ => unreachable!(),
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
            MessageBody::RequestBlock(index) => match self.blockchain.get_block(index)? {
                None => warn!("No block found at index: {index}"),
                Some(block) => self.sync_block(message.from, block)?,
            },
            MessageBody::SyncBlock(block) => {
                if self.blockchain.add_block(block)? {
                    let next_block_index = self.blockchain.len();
                    info!("request next block at index: {next_block_index}");
                    self.request_block_at(next_block_index)?;
                }
            }
            MessageBody::NewBlock(block) => {
                self.blockchain.add_block(block)?;
            }
            MessageBody::RequestAllValidators => {
                self.register_validator(self.peer_id)?;
            }
            MessageBody::RegisterValidator(validators) => {
                self.validators.insert(validators);
            }
            MessageBody::Vote(vote) => {
                if vote.index >= self.blockchain.len() {
                    self.insert_vote(message.from, vote);
                }
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
                    self.validators.remove(&peer);
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

    fn has_quorum(&self) -> bool {
        let validator_count = self.validators.len();
        let current_votes = self.current_index_votes().map(|m| m.len()).unwrap_or(0);
        validator_count >= MIN_VALIDATORS
            && (current_votes as f64 / validator_count as f64) >= MIN_VOTE_PERCENTAGE
    }

    fn current_index_votes(&self) -> Option<&HashMap<PeerId, PeerId>> {
        let current_index = self.blockchain.len();
        self.votes.get(&current_index)
    }

    fn choose_validator(&self) -> Option<PeerId> {
        let current_votes = self.current_index_votes()?;

        let mut votes = HashMap::new();
        for (_, to) in current_votes.iter() {
            votes.entry(to).and_modify(|e| *e += 1).or_insert(1);
        }

        votes
            .into_iter()
            // Flip tuple arguments so they are ordered first by votes
            // and then by peer_id.
            .map(|(peer_id, votes)| (votes, peer_id))
            .max()
            .map(|(_, peer_id)| *peer_id)
    }

    fn remove_deprecated_votes(&mut self) {
        let current_index = self.blockchain.len();
        self.votes.retain(|k, _| *k >= current_index);
    }

    fn insert_vote(&mut self, from: PeerId, vote: Vote) {
        self.votes
            .entry(vote.index)
            .and_modify(|m| {
                m.insert(from, vote.to);
            })
            .or_insert_with(|| {
                let mut map = HashMap::new();
                map.insert(from, vote.to);
                map
            });
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum TriggerEvent {
    RequestAllValidators,
    RegisterValidator,
    RequestBlock(usize),
    Vote,
    CreateBlock,
}

#[derive(Debug, Serialize, Deserialize)]
enum MessageBody {
    RequestBlock(usize),
    SyncBlock(Block),
    RequestAllValidators,
    RegisterValidator(PeerId),
    Vote(Vote),
    NewBlock(Block),
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
    keep_alive: keep_alive::Behaviour,
}

#[derive(Debug)]
enum MarkChainEvent {
    Floodsub(FloodsubEvent),
    Mdns(MdnsEvent),
    Void(void::Void),
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

impl From<void::Void> for MarkChainEvent {
    fn from(event: void::Void) -> MarkChainEvent {
        MarkChainEvent::Void(event)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
struct Vote {
    to: PeerId,
    index: usize,
}
