use bytemuck::{Pod, Zeroable};
use solana_program::pubkey::Pubkey;

use crate::utils::{impl_account_from_bytes, impl_to_bytes, Discriminator};

use super::AccountDiscriminator;

/// Proof accounts track a miner's current hash, claimable rewards, and lifetime stats.
/// Every miner is allowed one proof account which is required by the program to mine or claim rewards.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Tool {
    /// The tool authority.
    pub authority: Pubkey,

    /// Miner can is authorized to use the tool.
    pub miner: Pubkey,

    /// The equipped tool.
    pub asset: Pubkey,

    /// The remaining durability of the tool.
    pub durability: u64,

    /// The multiplier of the tool.
    pub multiplier: u64,
}

impl Discriminator for Tool {
    fn discriminator() -> u8 {
        AccountDiscriminator::Tool.into()
    }
}

impl_to_bytes!(Tool);
impl_account_from_bytes!(Tool);
