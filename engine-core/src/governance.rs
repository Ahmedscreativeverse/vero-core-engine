use crate::circuit_breaker::assert_closed;
use crate::event_struct::{ACT_APPROVE, ACT_EXECUTE, ACT_PROPOSE, MOD_GOV};
use crate::event_utils::publish_event;
use crate::types::{Proposal, ProposalState};
use soroban_sdk::{
    contracterror, panic_with_error, symbol_short, vec, Address, BytesN, Env, Map, Symbol, Vec,
};

const KEY_SIGNERS: Symbol = symbol_short!("SIGNERS");
const KEY_THRESH: Symbol = symbol_short!("THRESH");
const KEY_PROPOSALS: Symbol = symbol_short!("PROPS");

pub const TIMELOCK_LEDGERS: u32 = 720;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum GovError {
    NotASigner = 1,
    AlreadyApproved = 2,
    ProposalNotFound = 3,
    TimelockActive = 4,
    InvalidStateTransition = 5,
    InvalidThreshold = 6,
    DuplicateSigner = 7,
    DuplicateProposal = 8,
    AlreadyInitialized = 9,
}

pub fn init(env: &Env, signers: Vec<Address>, threshold: u32) {
    if env.storage().instance().has(&KEY_SIGNERS) {
        panic_with_error!(env, GovError::AlreadyInitialized);
    }
    if threshold == 0 || threshold > signers.len() {
        panic_with_error!(env, GovError::InvalidThreshold);
    }

    let mut seen = Vec::new(env);
    for signer in signers.iter() {
        if seen.contains(&signer) {
            panic_with_error!(env, GovError::DuplicateSigner);
        }
        seen.push_back(signer);
    }

    env.storage().instance().set(&KEY_SIGNERS, &signers);
    env.storage().instance().set(&KEY_THRESH, &threshold);

    let empty: Map<u64, (Proposal, u32)> = Map::new(env);
    env.storage().instance().set(&KEY_PROPOSALS, &empty);
}

pub fn signers(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&KEY_SIGNERS)
        .unwrap_or(vec![env])
}

pub fn threshold(env: &Env) -> u32 {
    env.storage().instance().get(&KEY_THRESH).unwrap_or(1)
}

pub fn load_proposals(env: &Env) -> Map<u64, (Proposal, u32)> {
    env.storage()
        .instance()
        .get(&KEY_PROPOSALS)
        .unwrap_or(Map::new(env))
}

fn save_proposals(env: &Env, proposals: &Map<u64, (Proposal, u32)>) {
    env.storage().instance().set(&KEY_PROPOSALS, proposals);
}

fn require_signer(env: &Env, signer: &Address) {
    let signers = signers(env);
    if !signers.contains(signer) {
        panic_with_error!(env, GovError::NotASigner);
    }
}

pub fn propose(env: &Env, mut proposal: Proposal) -> u64 {
    crate::non_reentrant!(env);
    assert_closed(env);
    require_signer(env, &proposal.proposer);
    proposal.proposer.require_auth();

    let mut proposals = load_proposals(env);
    if proposals.contains_key(proposal.id) {
        panic_with_error!(env, GovError::DuplicateProposal);
    }

    proposal.state = ProposalState::Pending;
    proposal.approved_by = vec![env];
    let unlock_ledger = env.ledger().sequence() + TIMELOCK_LEDGERS;
    proposals.set(proposal.id, (proposal.clone(), unlock_ledger));
    save_proposals(env, &proposals);

    publish_event(
        env,
        MOD_GOV | ACT_PROPOSE,
        proposal.id,
        BytesN::from_array(env, &[0u8; 32]),
    );
    proposal.id
}

pub fn approve(env: &Env, signer: &Address, proposal_id: u64) {
    crate::non_reentrant!(env);
    assert_closed(env);
    require_signer(env, signer);
    signer.require_auth();

    let mut proposals = load_proposals(env);
    let (mut proposal, unlock_ledger) = proposals
        .get(proposal_id)
        .unwrap_or_else(|| panic_with_error!(env, GovError::ProposalNotFound));

    if proposal.state != ProposalState::Pending {
        panic_with_error!(env, GovError::InvalidStateTransition);
    }
    if proposal.approved_by.contains(signer) {
        panic_with_error!(env, GovError::AlreadyApproved);
    }

    proposal.approved_by.push_back(signer.clone());
    let threshold = threshold(env);
    if proposal.approved_by.len() as u32 >= threshold {
        proposal.state = ProposalState::Approved;
    }

    proposals.set(proposal_id, (proposal.clone(), unlock_ledger));
    save_proposals(env, &proposals);
    publish_event(
        env,
        MOD_GOV | ACT_APPROVE,
        proposal_id,
        BytesN::from_array(env, &[0u8; 32]),
    );
}

pub fn execute(env: &Env, proposal_id: u64) -> Proposal {
    crate::non_reentrant!(env);
    assert_closed(env);

    let mut proposals = load_proposals(env);
    let (mut proposal, unlock_ledger) = proposals
        .get(proposal_id)
        .unwrap_or_else(|| panic_with_error!(env, GovError::ProposalNotFound));

    if proposal.state != ProposalState::Approved {
        panic_with_error!(env, GovError::InvalidStateTransition);
    }
    if env.ledger().sequence() < unlock_ledger {
        panic_with_error!(env, GovError::TimelockActive);
    }

    proposal.state = ProposalState::Executed;
    proposals.set(proposal_id, (proposal.clone(), unlock_ledger));
    save_proposals(env, &proposals);
    publish_event(
        env,
        MOD_GOV | ACT_EXECUTE,
        proposal_id,
        BytesN::from_array(env, &[0u8; 32]),
    );
    proposal
}

pub fn get_proposal(env: &Env, proposal_id: u64) -> Proposal {
    let proposals = load_proposals(env);
    let (proposal, _) = proposals
        .get(proposal_id)
        .unwrap_or_else(|| panic_with_error!(env, GovError::ProposalNotFound));
    proposal
}

pub fn get_unlock_ledger(env: &Env, proposal_id: u64) -> Option<u32> {
    load_proposals(env)
        .get(proposal_id)
        .map(|(_, unlock)| unlock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, vec, Address, BytesN, Env};

    #[contract]
    pub struct TestContract;

    #[contractimpl]
    impl TestContract {}

    fn proposal(env: &Env, id: u64, proposer: Address) -> Proposal {
        Proposal {
            id,
            action_hash: BytesN::from_array(env, &[7u8; 32]),
            proposer: proposer.clone(),
            approved_by: vec![env],
            state: ProposalState::Pending,
        }
    }

    #[test]
    fn proposal_initial_state_is_pending() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let proposer = Address::generate(&env);
        env.as_contract(&contract_id, || {
            init(&env, vec![&env, proposer.clone()], 1);
            let id = propose(&env, proposal(&env, 1, proposer.clone()));
            let stored = get_proposal(&env, id);
            assert_eq!(stored.state, ProposalState::Pending);
        });
    }

    #[test]
    fn pending_moves_to_approved_at_threshold() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let proposer = Address::generate(&env);
        env.as_contract(&contract_id, || {
            init(&env, vec![&env, proposer.clone()], 1);
            let id = propose(&env, proposal(&env, 2, proposer.clone()));
            approve(&env, &proposer, id);
            let stored = get_proposal(&env, id);
            assert_eq!(stored.state, ProposalState::Approved);
        });
    }

    #[test]
    fn approved_moves_to_executed_after_timelock() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let proposer = Address::generate(&env);
        env.as_contract(&contract_id, || {
            init(&env, vec![&env, proposer.clone()], 1);
            let id = propose(&env, proposal(&env, 3, proposer.clone()));
            approve(&env, &proposer, id);
            env.ledger().set_sequence_number(TIMELOCK_LEDGERS + 1);
            let executed = execute(&env, id);
            assert_eq!(executed.state, ProposalState::Executed);
        });
    }

    #[test]
    #[should_panic]
    fn double_approval_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let proposer = Address::generate(&env);
        env.as_contract(&contract_id, || {
            init(&env, vec![&env, proposer.clone()], 1);
            let id = propose(&env, proposal(&env, 4, proposer.clone()));
            approve(&env, &proposer, id);
            approve(&env, &proposer, id);
        });
    }
}
