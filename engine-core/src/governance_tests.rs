//! FSM verification tests for governance proposal state transitions.
//!
//! This module validates that the proposal state machine enforces valid
//! transitions and rejects invalid state transition attempts.

#[cfg(test)]
mod tests {
    use crate::governance::{self, GovError};
    use crate::types::{Proposal, ProposalState};
    use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, vec, Address, BytesN, Env, contract, contractimpl};

    #[contract]
    pub struct TestContract;

    #[contractimpl]
    impl TestContract {}

    fn setup_env(env: &Env) -> Address {
        env.register_contract(None, TestContract)
    }

    fn create_dummy_proposal(env: &Env, proposer: &Address) -> Proposal {
        Proposal {
            id: 1,
            action_hash: BytesN::from_array(env, &[0u8; 32]),
            proposer: proposer.clone(),
            approved_by: vec![env],
            state: ProposalState::Pending,
        }
    }

    /// Test: Proposal starts in Pending state
    #[test]
    fn test_proposal_initial_state_pending() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);
            let (p, _) = governance::load_proposals(&env).get(id).unwrap();
            assert_eq!(p.state, ProposalState::Pending);
        });
    }

    /// Test: Pending → Approved transition on threshold met
    #[test]
    fn test_state_transition_pending_to_approved() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);

            governance::approve(&env, &proposer, id);

            let (p, _) = governance::load_proposals(&env).get(id).unwrap();
            assert_eq!(p.state, ProposalState::Approved);
        });
    }

    /// Test: Approved → Executed transition on timelock expiry
    #[test]
    fn test_state_transition_approved_to_executed() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);

            governance::approve(&env, &proposer, id);

            // Fast forward ledger
            env.ledger().with_mut(|l| l.sequence_number = 1000);

            governance::execute(&env, id);

            let (p, _) = governance::load_proposals(&env).get(id).unwrap();
            assert_eq!(p.state, ProposalState::Executed);
        });
    }

    /// Test: Rejecting approvals on Approved proposals (invalid transition)
    #[test]
    #[should_panic]
    fn test_reject_approval_on_approved_proposal() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);
        let signer2 = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone(), signer2.clone()], 1);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);

            governance::approve(&env, &proposer, id);

            governance::approve(&env, &signer2, id);
        });
    }

    /// Test: Rejecting execution of Pending proposals
    #[test]
    #[should_panic]
    fn test_reject_execution_of_pending_proposal() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 2);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);

            governance::execute(&env, id);
        });
    }

    /// Test: Rejecting double-execution of Executed proposals
    #[test]
    #[should_panic]
    fn test_reject_double_execution() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);

            governance::approve(&env, &proposer, id);
            env.ledger().with_mut(|l| l.sequence_number = 1000);
            governance::execute(&env, id);

            governance::execute(&env, id);
        });
    }

    /// Test: Rejecting approval of Executed proposals
    #[test]
    #[should_panic]
    fn test_reject_approval_of_executed_proposal() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);
        let signer2 = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone(), signer2.clone()], 1);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);

            governance::approve(&env, &proposer, id);
            env.ledger().with_mut(|l| l.sequence_number = 1000);
            governance::execute(&env, id);

            governance::approve(&env, &signer2, id);
        });
    }

    /// Test: Full lifecycle - Pending → Approved → Executed
    #[test]
    fn test_full_proposal_lifecycle() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);

            let (p1, _) = governance::load_proposals(&env).get(id).unwrap();
            assert_eq!(p1.state, ProposalState::Pending);

            governance::approve(&env, &proposer, id);
            let (p2, _) = governance::load_proposals(&env).get(id).unwrap();
            assert_eq!(p2.state, ProposalState::Approved);

            env.ledger().with_mut(|l| l.sequence_number = 1000);
            governance::execute(&env, id);
            let (p3, _) = governance::load_proposals(&env).get(id).unwrap();
            assert_eq!(p3.state, ProposalState::Executed);
        });
    }

    /// Test: Error code for invalid transitions
    #[test]
    fn test_invalid_transition_error_code() {
        assert_eq!(GovError::InvalidStateTransition as u32, 5);
    }

    /// Test: Duplicate approval check still works after state changes
    #[test]
    #[should_panic]
    fn test_duplicate_approval_detection() {
        let env = Env::default();
        let contract_id = setup_env(&env);
        let proposer = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 2);
            let prop = create_dummy_proposal(&env, &proposer);
            let id = governance::propose(&env, prop);

            governance::approve(&env, &proposer, id);

            governance::approve(&env, &proposer, id);
        });
    }
}

/// State Transition Matrix (for documentation)
///
/// | Current State | Operation | Target State | Allowed | Error |
/// |---|---|---|---|---|
/// | Pending | approve (< threshold) | Pending | Yes | — |
/// | Pending | approve (>= threshold) | Approved | Yes | — |
/// | Pending | execute | — | No | InvalidStateTransition |
/// | Approved | approve | — | No | InvalidStateTransition |
/// | Approved | execute (timelock OK) | Executed | Yes | — |
/// | Approved | execute (timelock active) | — | No | TimelockActive |
/// | Executed | approve | — | No | InvalidStateTransition |
/// | Executed | execute | — | No | InvalidStateTransition |
pub struct StateTransitionMatrix;
