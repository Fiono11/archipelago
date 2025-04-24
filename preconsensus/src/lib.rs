use std::hash::Hash;

use rsnano_core::{BlockHash, BlockHashBuilder};

const FRONTIERS_THRESHOLD: usize = 1000;

// For a preproposal to be valid:
// - The length must be equal to FRONTIERS_THRESHOLD
// - It must contain only valid final voted blocks
// - The preproposal needs to be reliably broadcast, i.e, a preproposal needs to be echoed by at least 2f+1 nodes
struct PreProposal {
    frontiers: Vec<BlockHash>
}

impl PreProposal {
    fn hash(&self) -> BlockHash {
        let mut hasher =     BlockHashBuilder::new();
        for frontier in &self.frontiers {
            hasher = hasher.update(frontier.as_bytes());
        }
        hasher.build()
    }
}

// Fork of A and B
// Node 1 preproposes A because it has final voted it
// Node 2 preproposes A because it has final voted it
// Node 3 
// Node 4 (byzantine) preproposes B because it is byzantine

struct Proposal {
    // 2f+1 valid preproposals hashes
    // The proposal must contain all the frontiers that are included in at least f+1 preproposals
    preproposals: Vec<BlockHash>
}

impl Proposal {
    fn create_proposal(preproposals: Vec<PreProposal>) -> Proposal {
        Proposal {
            preproposals: preproposals.iter().map(|p| p.hash()).collect()
        }
    }
}

// Goal: Guarantee that every valid proposal contains all the blocks that were committed (final voted) by at least one correct node (f+1)
// This is because a block to be confirmed needs at least 2f+1 final votes
// So if a given block only has f final votes out of 2f+1 preproposals, it means it wasn't confirmed

#[test]
fn preproposal_hash() {
    let preproposal = PreProposal {
        frontiers: vec![BlockHash::from(1)]
    };

    assert_eq!(preproposal.hash(), BlockHash::decode_hex("33E423980C9B37D048BD5FADBD4A2AEB95146922045405ACCC2F468D0EF96988").unwrap());
}


#[test]
fn preproposal_to_proposal() {
    let preproposal = PreProposal {
        frontiers: vec![BlockHash::from(1)]
    };
    
    let hash = preproposal.hash();
    let proposal = Proposal::create_proposal(vec![preproposal]);

    assert_eq!(proposal.preproposals, vec![hash]);
}
