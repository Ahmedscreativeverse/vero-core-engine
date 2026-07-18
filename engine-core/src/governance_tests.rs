#[cfg(test)]
mod tests {
    use crate::governance;
    use crate::types::{Proposal, ProposalState};
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, vec, Address, BytesN, Env};

    #[contract]
    pub struct TestContract;

    #[contractimpl]
    impl TestContract {}

    fn proposal(env: &Env, id: u64, proposer: Address) -> Proposal {
        Proposal {
            id,
            action_hash: BytesN::from_array(env, &[7u8; 32]),
            proposer,
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
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let id = governance::propose(&env, proposal(&env, 1, proposer.clone()));
            let stored = governance::get_proposal(&env, id);

            assert_eq!(stored.state, ProposalState::Pending);
            assert_eq!(
                governance::get_unlock_ledger(&env, id),
                Some(governance::TIMELOCK_LEDGERS)
            );
        });
    }

    #[test]
    fn pending_moves_to_approved_at_threshold() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let proposer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let id = governance::propose(&env, proposal(&env, 2, proposer.clone()));

            governance::approve(&env, &proposer, id);

            assert_eq!(
                governance::get_proposal(&env, id).state,
                ProposalState::Approved
            );
        });
    }

    #[test]
    fn two_of_two_requires_both_approvals() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, alice.clone(), bob.clone()], 2);
            let id = governance::propose(&env, proposal(&env, 3, alice.clone()));

            governance::approve(&env, &alice, id);
            assert_eq!(
                governance::get_proposal(&env, id).state,
                ProposalState::Pending
            );

            governance::approve(&env, &bob, id);
            assert_eq!(
                governance::get_proposal(&env, id).state,
                ProposalState::Approved
            );
        });
    }

    #[test]
    fn approved_moves_to_executed_after_timelock() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let proposer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let id = governance::propose(&env, proposal(&env, 4, proposer.clone()));

            governance::approve(&env, &proposer, id);
            env.ledger()
                .set_sequence_number(governance::TIMELOCK_LEDGERS + 1);
            let executed = governance::execute(&env, id);

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
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let id = governance::propose(&env, proposal(&env, 5, proposer.clone()));

            governance::approve(&env, &proposer, id);
            governance::approve(&env, &proposer, id);
        });
    }

    #[test]
    #[should_panic]
    fn execution_before_timelock_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let proposer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            governance::init(&env, vec![&env, proposer.clone()], 1);
            let id = governance::propose(&env, proposal(&env, 6, proposer.clone()));

            governance::approve(&env, &proposer, id);
            governance::execute(&env, id);
        });
    }
}
