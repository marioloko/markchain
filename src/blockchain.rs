use crate::error::Result;

use multihash::{Multihash, MultihashDigest};
use serde::ser::Serialize;
use serde_derive::{Deserialize, Serialize};

use std::time::{SystemTime, UNIX_EPOCH};

/// The timestamp in millis for the genesis block. It is 2022-12-01 00:00:00.
const GENESIS_BLOCK_TIMESTAMP_MILLIS: u64 = 1669849200000;

/// The index of the genesis block.
const GENESIS_BLOCK_INDEX: usize = 0;

#[derive(Debug)]
pub struct BlockChain {
    blocks: Vec<Block>,
}

impl BlockChain {
    pub fn try_new() -> Result<BlockChain> {
        Ok(BlockChain {
            blocks: vec![generate_genesis_block()?],
        })
    }

    pub fn latest_block(&self) -> &Block {
        match self.blocks.last() {
            Some(block) => block,
            _ => unreachable!(),
        }
    }

    pub fn add_block(&mut self, new_block: Block) -> Result<()> {
        let previous_block = self.latest_block();
        let previous_header_hash = BlockHeader::generate_header_hash(&previous_block.header)?;
        if previous_block.header.index < new_block.header.index
            && self.blocks.len() == new_block.header.index
            && new_block.header.previous_header_hash == Some(previous_header_hash)
        {
            self.blocks.push(new_block);
        }
        Ok(())
    }

    pub fn generate_next_block(&self, transactions: Option<Vec<u8>>) -> Result<Block> {
        let latest_header = &self.latest_block().header;
        let previous_header_hash = BlockHeader::generate_header_hash(&latest_header)?;
        let index = latest_header.index + 1;
        let epoch_timestamp = millis_now();
        let merkle_root = BlockHeader::generate_merkle_root(&transactions)?;
        let header = BlockHeader::new(
            BlockVersion::V1,
            Some(previous_header_hash),
            index,
            merkle_root,
            epoch_timestamp,
        );
        Ok(Block::new(header, transactions))
    }

    pub fn get_block(&self, index: usize) -> Option<&Block> {
        self.blocks.get(index)
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockVersion {
    V1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    version: BlockVersion,
    previous_header_hash: Option<Multihash>,
    index: usize,
    merkle_root: Multihash,
    time: u64,
}

impl BlockHeader {
    fn new(
        version: BlockVersion,
        previous_header_hash: Option<Multihash>,
        index: usize,
        merkle_root: Multihash,
        time: u64,
    ) -> BlockHeader {
        BlockHeader {
            version,
            previous_header_hash,
            index,
            merkle_root,
            time,
        }
    }

    fn generate_header_hash(header: &BlockHeader) -> Result<Multihash> {
        sha3_256_multihash(header)
    }

    fn generate_merkle_root(transactions: &Option<Vec<u8>>) -> Result<Multihash> {
        sha3_256_multihash(transactions)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    header: BlockHeader,
    transactions: Option<Vec<u8>>,
}

impl Block {
    fn new(header: BlockHeader, transactions: Option<Vec<u8>>) -> Block {
        Block {
            header,
            transactions,
        }
    }
}

fn generate_genesis_block() -> Result<Block> {
    let transactions = None;
    let merkle_root = BlockHeader::generate_merkle_root(&transactions)?;
    let header = BlockHeader::new(
        BlockVersion::V1,
        None,
        GENESIS_BLOCK_INDEX,
        merkle_root,
        GENESIS_BLOCK_TIMESTAMP_MILLIS,
    );
    Ok(Block::new(header, transactions))
}

fn millis_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sha3_256_multihash<S>(data: &S) -> Result<Multihash>
where
    S: Serialize,
{
    let bytes = bincode::serialize(data)?;
    Ok(multihash::Code::Sha3_256.digest(&bytes))
}
