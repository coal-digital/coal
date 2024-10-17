use bytemuck::{Pod, Zeroable};
use solana_program::pubkey::Pubkey;

use crate::utils::{impl_account_from_bytes, impl_to_bytes, Discriminator};

use super::AccountDiscriminator;

/// Proof accounts track a miner's current hash, claimable rewards, and lifetime stats.
/// Every miner is allowed one proof account which is required by the program to mine or claim rewards.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Reprocessor {
    /// The reprocess authority.
    pub authority: Pubkey,
    /// The slot the reprocessor was created at.
    pub slot: u64,
    /// Sysvar hashes
    pub hash: [u8; 32],
}

impl Discriminator for Reprocessor {
    fn discriminator() -> u8 {
        AccountDiscriminator::Reprocessor.into()
    }
}

impl_to_bytes!(Reprocessor);
impl_account_from_bytes!(Reprocessor);
