use std::fs;
use std::path::Path;

use libp2p::identity::ed25519::{Keypair, PublicKey, SecretKey};

use crate::db::{ByteTree, Db};
use crate::error::Result;

/// The name of the database tree where to store wallets.
const WALLET_DB: &str = "wallets";

/// The name of the default wallet.
const DEFAULT_WALLET: &str = "__default__";

#[derive(Debug)]
pub struct WalletManager {
    wallets: ByteTree,
}

impl WalletManager {
    pub fn try_new(db: &Db) -> Result<WalletManager> {
        let wallets = db.open_byte_tree(WALLET_DB)?;

        if get_wallet(&wallets, DEFAULT_WALLET)?.is_none() {
            let wallet = Wallet::new();
            insert_wallet(&wallets, DEFAULT_WALLET, &wallet)?;
        }

        Ok(WalletManager { wallets })
    }

    pub fn get_wallet(&self, name: &str) -> Result<Option<Wallet>> {
        get_wallet(&self.wallets, name)
    }

    pub fn default_wallet(&self) -> Result<Wallet> {
        match self.get_wallet(DEFAULT_WALLET)? {
            Some(wallet) => Ok(wallet),
            None => unreachable!(),
        }
    }
}

#[inline]
fn get_wallet(wallets: &ByteTree, name: &str) -> Result<Option<Wallet>> {
    let wallet = match wallets.get(name)? {
        None => None,
        Some(mut bytes) => Some(Wallet::from(SecretKey::from_bytes(&mut bytes)?)),
    };

    Ok(wallet)
}

#[inline]
fn insert_wallet(wallets: &ByteTree, name: &str, wallet: &Wallet) -> Result<()> {
    let secret = wallet.keypair.secret();
    wallets.insert(name, secret.as_ref())?;

    Ok(())
}

pub type WalletAddress = PublicKey;

#[derive(Debug)]
pub struct Wallet {
    keypair: Keypair,
}

impl Wallet {
    pub fn new() -> Wallet {
        let keypair = Keypair::generate();
        Wallet { keypair }
    }

    pub fn dump<P: AsRef<Path>>(&self, path: &P) -> Result<()> {
        fs::write(path, self.keypair.secret().as_ref())?;
        Ok(())
    }

    pub fn load<P: AsRef<Path>>(&self, path: &P) -> Result<Wallet> {
        let mut bytes = fs::read(path)?;
        let secret = SecretKey::from_bytes(&mut bytes)?;
        let keypair = Keypair::from(secret);
        Ok(Wallet { keypair })
    }

    pub fn address(&self) -> WalletAddress {
        self.keypair.public()
    }
}

impl From<Keypair> for Wallet {
    fn from(keypair: Keypair) -> Wallet {
        Wallet { keypair }
    }
}

impl From<SecretKey> for Wallet {
    fn from(secret: SecretKey) -> Wallet {
        let keypair = Keypair::from(secret);
        Wallet { keypair }
    }
}
