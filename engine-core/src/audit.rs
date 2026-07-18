use crate::circuit_breaker::assert_closed;
use crate::event_struct::{ACT_COMMIT, MOD_AUDIT};
use crate::event_utils::{publish_event, publish_event_legacy};
use crate::types::StateCommitment;
use sha2::{Digest, Sha256};
use soroban_sdk::{contracterror, panic_with_error, symbol_short, BytesN, Env, Map, Symbol, Val};

const KEY_SEQ: Symbol = symbol_short!("SEQ");
const KEY_PREV: Symbol = symbol_short!("PREV_H");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum AuditError {
    ReplayedSequence = 1,
    HashMismatch = 2,
}

pub fn compute_commitment(prev_hash: &[u8; 32], sequence: u64, payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash);
    hasher.update(sequence.to_be_bytes());
    hasher.update(payload);
    hasher.finalize().into()
}

pub fn get_last_sequence(env: &Env) -> u64 {
    env.storage().instance().get(&KEY_SEQ).unwrap_or(0)
}

pub fn get_previous_hash_raw(env: &Env) -> [u8; 32] {
    env.storage()
        .instance()
        .get::<Symbol, [u8; 32]>(&KEY_PREV)
        .unwrap_or([0u8; 32])
}

pub fn get_state_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &get_previous_hash_raw(env))
}

pub fn integrity_check(env: &Env, commitment: &StateCommitment, payload: &[u8]) -> bool {
    if commitment.sequence <= get_last_sequence(env) {
        return false;
    }
    let expected = compute_commitment(&get_previous_hash_raw(env), commitment.sequence, payload);
    expected == commitment.state_hash.to_array()
}

pub fn last_sequence(env: &Env) -> u64 {
    get_last_sequence(env)
}

pub fn previous_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &get_previous_hash_raw(env))
}

pub fn validate_transition(env: &Env, commitment: &StateCommitment, payload: &[u8]) {
    crate::non_reentrant!(env);
    assert_closed(env);
    commitment.author.require_auth();

    if commitment.sequence <= get_last_sequence(env) {
        panic_with_error!(env, AuditError::ReplayedSequence);
    }

    let prev_hash = get_previous_hash_raw(env);
    let expected = compute_commitment(&prev_hash, commitment.sequence, payload);
    if expected != commitment.state_hash.to_array() {
        panic_with_error!(env, AuditError::HashMismatch);
    }

    env.storage().instance().set(&KEY_SEQ, &commitment.sequence);
    env.storage()
        .instance()
        .set(&KEY_PREV, &commitment.state_hash);

    publish_event(
        env,
        MOD_AUDIT | ACT_COMMIT,
        commitment.sequence,
        commitment.state_hash.clone(),
    );

    let mut payload_map: Map<Symbol, Val> = Map::new(env);
    payload_map.set(symbol_short!("seq"), commitment.sequence.into_val(env));
    payload_map.set(
        symbol_short!("hash"),
        commitment.state_hash.clone().into_val(env),
    );
    publish_event_legacy(
        env,
        BytesN::from_array(env, &[0u8; 32]),
        BytesN::from_array(env, &[0u8; 32]),
        payload_map,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env};

    #[contract]
    pub struct TestContract;

    #[contractimpl]
    impl TestContract {}

    fn commitment(env: &Env, author: Address, sequence: u64, payload: &[u8]) -> StateCommitment {
        let prev = get_previous_hash_raw(env);
        let hash = compute_commitment(&prev, sequence, payload);
        StateCommitment {
            state_hash: BytesN::from_array(env, &hash),
            sequence,
            ledger: env.ledger().sequence(),
            author,
        }
    }

    #[test]
    fn valid_first_commitment() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        env.as_contract(&contract_id, || {
            let payload = b"state_payload_v1";
            let c = commitment(&env, Address::generate(&env), 1, payload);
            validate_transition(&env, &c, payload);
            assert_eq!(last_sequence(&env), 1);
            assert_eq!(get_state_hash(&env), c.state_hash);
        });
    }

    #[test]
    #[should_panic]
    fn replay_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        env.as_contract(&contract_id, || {
            let payload = b"payload";
            let c = commitment(&env, Address::generate(&env), 1, payload);
            validate_transition(&env, &c, payload);
            validate_transition(&env, &c, payload);
        });
    }

    #[test]
    #[should_panic]
    fn hash_mismatch_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, TestContract);
        let author = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let payload = b"payload";
            let mut c = commitment(&env, author, 1, payload);
            c.state_hash = BytesN::from_array(&env, &[9u8; 32]);
            validate_transition(&env, &c, payload);
        });
    }
}
