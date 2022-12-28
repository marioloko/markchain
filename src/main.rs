use libp2p::identity;
use markchain::error::Result;
use markchain::p2p::MarkChainNetwork;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let keys = identity::Keypair::generate_ed25519();
    let mut network = MarkChainNetwork::new(keys)?;
    network.run().await
}
