use std::collections::{BTreeSet, HashMap};
use rsnano_core::{BlockHash, BlockHashBuilder};

const FRONTIERS_THRESHOLD: usize = 1000;
type ProposalHash = BlockHash;

// For a preproposal to be valid:
// - The length must be equal to FRONTIERS_THRESHOLD
// - It must contain only valid final voted blocks
// - The preproposal needs to be reliably broadcast, i.e, a preproposal needs to be echoed by at least 2f+1 nodes
#[derive(Clone)]
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
    preproposals: Vec<ProposalHash>
}

impl Proposal {
    fn create_proposal(preproposals: Vec<PreProposal>) -> Proposal {
        Proposal {
            preproposals: preproposals.iter().map(|p| p.hash()).collect()
        }
    }

    fn hash(&self) -> ProposalHash {
        let mut hasher = BlockHashBuilder::new();
        let preproposals: BTreeSet<ProposalHash> = self.preproposals.iter().cloned().collect();
        for preproposal in &preproposals {
            hasher = hasher.update(preproposal.as_bytes());
        }
        hasher.build()
    }
    
    /// Returns all frontiers that are included in at least f+1 preproposals
    /// f is the Byzantine fault tolerance parameter, typically (n-1)/3 for a system with n nodes
    fn frontiers(&self, all_preproposals: &[PreProposal], f: usize) -> Vec<BlockHash> {
        // Count occurrences of each frontier block across all preproposals
        let mut frontier_counts: HashMap<BlockHash, usize> = HashMap::new();
        
        for preproposal in all_preproposals {
            // Skip preproposals that are not part of this proposal
            if !self.preproposals.contains(&preproposal.hash()) {
                continue;
            }
            
            // Count each frontier block
            for frontier in &preproposal.frontiers {
                *frontier_counts.entry(*frontier).or_insert(0) += 1;
            }
        }
        
        // Keep only frontiers that appear in at least f+1 preproposals
        let threshold = f + 1;
        let mut result: Vec<BlockHash> = frontier_counts
            .into_iter()
            .filter(|(_, count)| *count >= threshold)
            .map(|(block_hash, _)| block_hash)
            .collect();
        
        // Sort for deterministic output
        result.sort();
        
        result
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

#[test]
fn proposal_hash_with_unordered_preproposals() {
    let preproposal1 = PreProposal {
        frontiers: vec![BlockHash::from(1)]
    };

    let preproposal2 = PreProposal {
        frontiers: vec![BlockHash::from(2)]
    };
    
    let proposal1 = Proposal::create_proposal(vec![preproposal1.clone(), preproposal2.clone()]);
    let proposal2 = Proposal::create_proposal(vec![preproposal2, preproposal1]);

    assert_eq!(proposal1.hash(), proposal2.hash());
}

#[test]
fn proposal_frontiers() {
    let block1 = BlockHash::from(1);
    let block2 = BlockHash::from(2);
    let block3 = BlockHash::from(3);

    // Node 1 has final voted block 1 and block 2
    let preproposal1 = PreProposal {
        frontiers: vec![BlockHash::from(1), BlockHash::from(2)]
    };
    
    // Node 2 has final voted block 1 
    let preproposal2 = PreProposal {
        frontiers: vec![BlockHash::from(1)]
    };
    
    // Node 3 has confirmed block 1 and final voted block 2
    let preproposal3 = PreProposal {
        frontiers: vec![BlockHash::from(1), BlockHash::from(2)]
    };
    
    // Node 4 is byzantine and preproposes block 3, which is a fork of block 2
    let preproposal4 = PreProposal {
        frontiers: vec![BlockHash::from(3)]
    };
    
    // Create a proposal that includes the preproposals from node 1, 2 and 4 
    let preproposals = vec![
        preproposal1.clone(), 
        preproposal2.clone(), 
        preproposal4.clone()
    ];

    let proposal = Proposal::create_proposal(preproposals.clone());
    let proposal_frontiers = proposal.frontiers(&preproposals, 1);
    
    // The confirmed block1 needs to be included in the proposal
    assert!(proposal_frontiers.contains(&block1));
}