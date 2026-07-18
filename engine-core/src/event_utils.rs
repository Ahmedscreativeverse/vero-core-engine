use crate::event_struct::CompactEvent;
use soroban_sdk::{symbol_short, BytesN, Env, Map, Symbol, Val};

/// Publish a deterministic compact event under a single topic for indexing.
pub fn publish_event(env: &Env, flags: u32, value: u64, hash: BytesN<32>) {
    let ev = CompactEvent { flags, value, hash };
    env.events()
        .publish((symbol_short!("EVENT"), symbol_short!("LOG")), ev);
}

/// Return the canonical all-zero hash used when an event has no hash payload.
pub fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

/// Compatibility function for legacy events.
pub fn publish_event_legacy(
    env: &Env,
    event_type: BytesN<32>,
    action: BytesN<32>,
    payload: Map<Symbol, Val>,
) {
    env.events().publish((event_type, action), payload);
}
