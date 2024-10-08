use std::mem::size_of;

use coal_api::{consts::*, error::CoalError, instruction::ReprocessArgs, loaders::*, state::{Reprocessor, Proof}};
use solana_program::{
    account_info::AccountInfo,
    clock::Clock, 
    entrypoint::ProgramResult, 
    msg,
    program_error::ProgramError, 
    slot_hashes::SlotHash, 
    sysvar::{self, Sysvar},
    keccak::hashv
};

use crate::utils::{create_pda, AccountDeserialize, Discriminator};


pub fn process_initialize_reprocess(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // Parse args.
    let args = ReprocessArgs::try_from_bytes(data)?;

    // Load accounts.
    let [signer, reprocessor_info, slot_hashes_sysvar, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    load_signer(signer)?;
    load_uninitialized_pda(
        reprocessor_info,
        &[REPROCESSOR, signer.key.as_ref()],
        args.reprocessor_bump,
        &coal_api::id(),
    )?;
    load_sysvar(slot_hashes_sysvar, sysvar::slot_hashes::id())?;

    // Initialize reprocessor.
    create_pda(
        reprocessor_info,
        &coal_api::id(),
        8 + size_of::<Reprocessor>(),
        &[REPROCESSOR, signer.key.as_ref(), &[args.reprocessor_bump]],
        system_program,
        signer,
    )?;

    let mut reprocessor_data = reprocessor_info.data.borrow_mut();
    reprocessor_data[0] = Reprocessor::discriminator() as u8;
    let reprocessor = Reprocessor::try_from_bytes_mut(&mut reprocessor_data)?;
    reprocessor.authority = *signer.key;
    
    let slot = Clock::get()?.slot;
    reprocessor.slot = slot;
    reprocessor.hash = hashv(&[
        &slot_hashes_sysvar.data.borrow()[0..size_of::<SlotHash>()],
    ])
    .0;

    Ok(())
}

pub fn process_finalize_reprocess(accounts: &[AccountInfo], _data: &[u8]) -> ProgramResult {
    // Parse args.
    // let args = ReprocessArgs::try_from_bytes(data)?;

    // Load accounts.
    let [signer, reprocessor_info, proof_info, slot_hashes_sysvar] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    load_signer(signer)?;
    msg!("loaded signer");
    load_coal_proof(proof_info, signer.key, true)?;
    msg!("loaded coal proof");
    // TODO: load reprocessor
    load_sysvar(slot_hashes_sysvar, sysvar::slot_hashes::id())?;
    msg!("loaded program");

    
    let mut reprocessor_data = reprocessor_info.data.borrow_mut();
    let reprocessor = Reprocessor::try_from_bytes_mut(&mut reprocessor_data)?;
    
    // Target slot is 100 slots ahead of the starting slot
    let target_slot = reprocessor.slot + 100;
    
    let current_slot = Clock::get()?.slot;
    
    // Check if the current slot is less than the target slot
    if current_slot.le(&target_slot) {
        return Err(CoalError::SlotTooEarly.into());
    }
    
    let mut proof_data = proof_info.data.borrow_mut();
    let proof = Proof::try_from_bytes_mut(&mut proof_data)?;
    let total_hashes = proof.total_hashes;
    let total_rewards = proof.total_rewards;

    if total_hashes.eq(&0) || total_rewards.eq(&0) {
        return Err(CoalError::Spam.into())
    }
    // Reset total hashes abd rewards to 0
    proof.total_hashes = 0;
    proof.total_rewards = 0;

    // Calculate the final hash
    let final_hash = hashv(&[
        &reprocessor.hash,
        &slot_hashes_sysvar.data.borrow()[0..size_of::<SlotHash>()],
    ])
    .0;

    // Derive a number between 1 and 100 from the final hash
    let pseudo_random_number = derive_number_from_hash(&final_hash, 1, 100);
    msg!("Derived number: {}", pseudo_random_number);
    msg!("Total hashes: {}", total_hashes);
    let mut reward = ONE_COAL.saturating_div(10_000).saturating_div(pseudo_random_number as u64);
    msg!("Initial reward: {}", reward);
    // Weighted by total hashes and total rewards
    let hash_weight = 0.6; // 60% weight to hashes
    let reward_weight = 0.4; // 40% weight to rewards
    
    let hash_factor = (total_hashes as f64) / (total_hashes as f64 + total_rewards as f64);
    let reward_factor = (total_rewards as f64) / (total_hashes as f64 + total_rewards as f64);
    msg!("Hash factor: {}", hash_factor);
    msg!("Reward factor: {}", reward_factor);
    
    let weighted_factor = (hash_factor * hash_weight + reward_factor * reward_weight) as f64;
    msg!("Weighted factor: {}", weighted_factor);
    reward = (reward as f64 * weighted_factor) as u64;
    msg!("Weighted reward: {} ", reward);
    // Calculate the liveness penalty
    let s_buffer = 5;
    let s_tolerance = target_slot.saturating_add(s_buffer);

    if current_slot.gt(&s_tolerance) {
        // Halve the reward for every slot late.
        let halvings = current_slot.saturating_sub(s_tolerance) as u64;
        msg!("Halvings: {}", halvings);
        if halvings.gt(&0) {
            reward = reward.saturating_div(2u64.saturating_pow(halvings as u32));
        }
    }
    msg!("Reward: {}", reward);
    msg!("Reward in UNITS: {}", (reward as f64) / 10f64.powf(TOKEN_DECIMALS as f64));
    drop(reprocessor_data);

    // Realloc data to zero.
    reprocessor_info.realloc(0, true)?;

    // Send remaining lamports to signer.
    **signer.lamports.borrow_mut() += reprocessor_info.lamports();
    **reprocessor_info.lamports.borrow_mut() = 0;

    Ok(())

}

// Helper function to derive a number from the entire hash
fn derive_number_from_hash(hash: &[u8; 32], min: u64, max: u64) -> u64 {
    let mut acc = 0u64;
    for chunk in hash.chunks(8) {
        acc = acc.wrapping_add(u64::from_le_bytes(chunk.try_into().unwrap_or([0; 8])));
    }
    min + (acc % (max - min + 1))
}