//! Shared defensive guards for state-mutating entry points.
//!
//! The reentrancy guard stores a temporary lock for the duration of a call.
//! Any nested attempt to enter another guarded state-mutating function in the
//! same invocation observes the lock and reverts with `ReentrancyDetected`.

use soroban_sdk::{contracterror, panic_with_error, symbol_short, Env, Symbol};

const KEY_REENTRANCY_LOCK: Symbol = symbol_short!("R_LOCK");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GuardError {
    ReentrancyDetected = 1,
}

/// RAII lock that clears the temporary reentrancy flag on normal function exit.
pub struct ReentrancyGuard<'a> {
    env: &'a Env,
}

impl<'a> ReentrancyGuard<'a> {
    /// Enter a non-reentrant section, reverting if the current invocation is already locked.
    pub fn enter(env: &'a Env) -> Self {
        if env.storage().temporary().has(&KEY_REENTRANCY_LOCK) {
            panic_with_error!(env, GuardError::ReentrancyDetected);
        }

        env.storage().temporary().set(&KEY_REENTRANCY_LOCK, &true);
        Self { env }
    }
}

impl Drop for ReentrancyGuard<'_> {
    fn drop(&mut self) {
        self.env.storage().temporary().remove(&KEY_REENTRANCY_LOCK);
    }
}

/// Execute a block while holding the shared reentrancy lock.
#[macro_export]
macro_rules! non_reentrant {
    ($env:expr, $body:block) => {{
        let _reentrancy_guard = $crate::guards::ReentrancyGuard::enter($env);
        $body
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::contract;

    #[contract]
    struct TestContract;

    #[test]
    #[should_panic]
    fn nested_entry_reverts() {
        let env = Env::default();
        let contract_id = env.register_contract(None, TestContract);
        env.as_contract(&contract_id, || {
            let _guard = ReentrancyGuard::enter(&env);

            let _nested_guard = ReentrancyGuard::enter(&env);
        });
    }

    #[test]
    fn lock_clears_after_scope_exit() {
        let env = Env::default();
        let contract_id = env.register_contract(None, TestContract);
        env.as_contract(&contract_id, || {
            {
                let _guard = ReentrancyGuard::enter(&env);
            }

            let _guard = ReentrancyGuard::enter(&env);
        });
    }
}
