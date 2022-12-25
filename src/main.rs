use markchain::error::Result;
use markchain::p2p::MarkChainNetwork;

#[tokio::main]
async fn main() -> Result<()> {
    let mut network = MarkChainNetwork::new()?;
    network.run().await
}
