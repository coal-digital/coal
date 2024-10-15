use std::mem::size_of;

use coal_api::{
    consts::*,
    error::CoalError,
    loaders::*,
    state::{Bus, Proof, Reprocessor}
};
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

use crate::utils::AccountDeserialize;

pub fn process_finalize_reprocess(accounts: &[AccountInfo], _data: &[u8]) -> ProgramResult {
    // Load accounts.
    let [signer, reprocessor_info, proof_info, bus_info, mint_info, tokens_info, treasury_info, token_program, slot_hashes_sysvar] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    load_signer(signer)?;
    load_coal_bus(bus_info, 0, false)?;
    load_coal_proof(proof_info, signer.key, true)?;
    load_reprocessor(reprocessor_info, signer.key, true)?;
    load_sysvar(slot_hashes_sysvar, sysvar::slot_hashes::id())?;

    
    let mut reprocessor_data = reprocessor_info.data.borrow_mut();
    let reprocessor = Reprocessor::try_from_bytes_mut(&mut reprocessor_data)?;
    
    // Target slot is 100 slots ahead of the starting slot
    let target_slot = reprocessor.slot;
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

    // IMPORTANT: Reset total hashes and rewards to 0
    proof.total_hashes = 0;
    proof.total_rewards = 0;

    // Calculate the final hash
    let bus_data = bus_info.data.borrow();
    let bus = Bus::try_from_bytes(&bus_data)?;
    let final_hash = hashv(&[
        &reprocessor.hash,
        &bus.rewards.to_le_bytes(),
        &slot_hashes_sysvar.data.borrow()[0..size_of::<SlotHash>()],
    ])
    .0;

    // Derive a number between 1 and 100 from the final hash
    let pseudo_random_number = derive_number_from_hash(&final_hash, 1, 100);
    msg!("Derived number: {}", pseudo_random_number);
    let mut reward = calculate_reward(total_hashes, total_rewards, pseudo_random_number);

    // Calculate the liveness penalty
    let s_buffer = 5;
    let s_tolerance = target_slot.saturating_add(s_buffer);
    
    msg!("Current slot: {}", current_slot);
    msg!("Target slot: {}", target_slot);
    
    if current_slot.gt(&s_tolerance) {
        // Halve the reward for every slot late.
        let halvings = current_slot.saturating_sub(s_tolerance) as u64;
        msg!("Halvings: {}", halvings);
        if halvings.gt(&0) {
            reward = reward.saturating_div(2u64.saturating_pow(halvings as u32));
        }
    }

    // Mint chromium rewards
    solana_program::program::invoke_signed(
        &spl_token::instruction::mint_to(
            &spl_token::id(),
            mint_info.key,
            tokens_info.key,
            treasury_info.key,
            &[treasury_info.key],
            reward,
        )?,
        &[
            token_program.clone(),
            mint_info.clone(),
            tokens_info.clone(),
            treasury_info.clone(),
        ],
        &[&[TREASURY, &[TREASURY_BUMP]]],
    )?;

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

fn calculate_reward(total_hashes: u64, total_rewards: u64, pseudo_random_number: u64) -> u64 {    
    // Calculate a hash factor (gives more weight to number of hashes)
    let scaling_factor = BASE_COAL_REWARD_RATE_MIN_THRESHOLD;
    let hash_factor = scaling_factor.saturating_mul(total_hashes);
    
    // Calculate a reward factor (gives some weight to total rewards)
    let reward_factor = (total_rewards as f64).cbrt() as u64;
    
    // Combine factors
    let combined_factor = hash_factor.saturating_mul(reward_factor);

    let min_reward = combined_factor;
    let max_reward = min_reward.saturating_mul(100);

    // Logs
    msg!("Total hashes: {}", total_hashes);
    msg!("Total rewards: {}", total_rewards);
    msg!("Min reward: {}", min_reward);
    msg!("Max reward: {}", max_reward);
    
    // Apply randomness
    combined_factor.saturating_mul(pseudo_random_number)
}