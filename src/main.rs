use std::path::PathBuf;

use libp2p::identity;
use markchain::error::Result;
use markchain::p2p::MarkChainNetwork;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "markchain", about = "A toy blockchain for didactic purposes")]
struct Args {
    #[structopt(short = "d", long = "database-path")]
    database_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::from_args();

    env_logger::init();

    let keys = identity::Keypair::generate_ed25519();
    let mut network = MarkChainNetwork::try_new(keys, &args.database_path)?;
    network.run().await
}
