use std::path::PathBuf;

use libp2p::futures::StreamExt;
use libp2p::identity;
use log::error;
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
    let network = MarkChainNetwork::try_new(keys, &args.database_path)?;

    let mut event_reader = network.into_event_stream()?.boxed();
    while let Some(result) = event_reader.next().await {
        if let Err(error) = result {
            error!("{}", error);
        }
    }

    Ok(())
}
