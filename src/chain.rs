use crate::block::{Block, BlockHeader, BlockVersion};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct BlockChain {
    blocks: Vec<Block>,
}

impl BlockChain {
    pub fn new() -> BlockChain {
        BlockChain {
            blocks: vec![gen_genesis_block()],
        }
    }

    pub fn latest_block(&self) -> &Block {
        match self.blocks.last() {
            Some(block) => block,
            _ => unreachable!(),
        }
    }

    pub fn add_block(&mut self, new_block: Block) {
        let previous_block = self.latest_block();
        if previous_block.index() < new_block.index()
            && new_block.header().previous_block_header()
                == Some(previous_block.header().merkle_root())
        {
            self.blocks.push(new_block);
        }
    }

    pub fn get_block(&self, index: usize) -> Option<&Block> {
        self.blocks.get(index)
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }
}

fn gen_genesis_block() -> Block {
    let epoch_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let header = BlockHeader::new(
        BlockVersion::V1,
        None,
        "0x1bc33000000000 00000000000000000000000000000000000".into(),
        epoch_timestamp,
    );
    Block::new(header, 0, None)
}
