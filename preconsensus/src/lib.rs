use std::{collections::BTreeSet, hash::Hash};

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
        let mut hasher = BlockHashBuilder::new();
        let frontiers: BTreeSet<BlockHash> = self.frontiers.iter().cloned().collect();
        for frontier in &frontiers {
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

    fn hash(&self) -> BlockHash {
        let mut hasher = BlockHashBuilder::new();
        for preproposal in &self.preproposals {
            hasher = hasher.update(preproposal.as_bytes());
        }
        hasher.build()
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
fn preproposal_hash_with_unordered_frontiers() {
    let preproposal1 = PreProposal {
        frontiers: vec![BlockHash::from(1), BlockHash::from(2)]
    };

    let preproposal2 = PreProposal {
        frontiers: vec![BlockHash::from(2), BlockHash::from(1)]
    };

    assert_eq!(preproposal1.hash(), preproposal2.hash());
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

#[test]
fn preproposals_to_proposal() {
    let preproposal1 = PreProposal {
        frontiers: vec![BlockHash::from(1)]
    };

    let preproposal2 = PreProposal {
        frontiers: vec![BlockHash::from(2)]
    };
    
    let hash1 = preproposal1.hash();
    let hash2 = preproposal2.hash();
    let proposal = Proposal::create_proposal(vec![preproposal1, preproposal2]);

    assert_eq!(proposal.preproposals, vec![hash1, hash2]);
}

#[test]
fn proposal_hash() {
    let preproposal = PreProposal {
        frontiers: vec![BlockHash::from(1)]
    };
    
    let proposal = Proposal::create_proposal(vec![preproposal]);

    assert_eq!(proposal.hash(), BlockHash::decode_hex("F7DB7E88D5E925085FED1B0FE3D63FC013F6A7339E1027573239A2AD767998A4").unwrap());
}