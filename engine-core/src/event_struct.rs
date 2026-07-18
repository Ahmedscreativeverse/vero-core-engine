//! Compact event encoding for audit-friendly Soroban logs.

use soroban_sdk::{contracttype, BytesN};

pub const MOD_AUDIT: u32 = 0x01;
pub const MOD_GOV: u32 = 0x02;
pub const MOD_TREASURY: u32 = 0x03;
pub const MOD_CB: u32 = 0x04;
pub const MOD_BURN: u32 = 0x05;
pub const MOD_RECOVERY: u32 = 0x06;
pub const MOD_FEE: u32 = 0x07;
pub const MOD_CORE: u32 = 0x08;
pub const MOD_UPGRADE: u32 = 0x09;

pub const ACT_COMMIT: u32 = 0x01 << 8;
pub const ACT_SNAPSHOT: u32 = 0x02 << 8;
pub const ACT_PROPOSE: u32 = 0x03 << 8;
pub const ACT_APPROVE: u32 = 0x04 << 8;
pub const ACT_EXECUTE: u32 = 0x05 << 8;
pub const ACT_TRIP: u32 = 0x06 << 8;
pub const ACT_RESET: u32 = 0x07 << 8;
pub const ACT_BURN_SAFE: u32 = 0x08 << 8;
pub const ACT_REQUEST: u32 = 0x09 << 8;
pub const ACT_TRIGGERED: u32 = 0x0A << 8;
pub const ACT_FEE: u32 = 0x0B << 8;
pub const ACT_INIT: u32 = 0x0C << 8;
pub const ACT_TRANSITION: u32 = 0x0D << 8;
pub const ACT_UPGRADE: u32 = 0x0E << 8;
pub const ACT_UPDATE: u32 = 0x0F << 8;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactEvent {
    pub flags: u32,
    pub value: u64,
    pub hash: BytesN<32>,
}
