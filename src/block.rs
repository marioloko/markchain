use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockVersion {
    V1,
    V2,
    V3,
    V4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    version: BlockVersion,
    previous_block_header: Option<String>,
    merkle_root: String,
    time: u64,
}

impl BlockHeader {
    pub fn new(
        version: BlockVersion,
        previous_block_header: Option<String>,
        merkle_root: String,
        time: u64,
    ) -> BlockHeader {
        BlockHeader {
            version,
            previous_block_header,
            merkle_root,
            time,
        }
    }

    pub fn previous_block_header(&self) -> Option<&str> {
        self.previous_block_header.as_deref()
    }

    pub fn merkle_root(&self) -> &str {
        &self.merkle_root
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    header: BlockHeader,
    index: u64,
    txns: Option<Vec<u8>>,
}

impl Block {
    pub fn new(header: BlockHeader, index: u64, txns: Option<Vec<u8>>) -> Block {
        Block {
            header,
            index,
            txns,
        }
    }

    pub fn index(&self) -> u64 {
        self.index
    }

    pub fn header(&self) -> &BlockHeader {
        &self.header
    }
}
