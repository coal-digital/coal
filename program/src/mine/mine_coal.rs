use std::mem::size_of;

use drillx::Solution;
use coal_api::{
    consts::*, error::CoalError, event::MineEvent, guild_loaders::{load_guild_config, load_guild_with_member, load_member}, instruction::MineArgs, loaders::*, state::{Bus, Config, Proof, Tool}
};
use solana_program::msg;
#[allow(deprecated)]
use solana_program::{
    account_info::AccountInfo,
    clock::Clock,
    entrypoint::ProgramResult,
    keccak::hashv,
    program::set_return_data,
    program_error::ProgramError,
    pubkey::Pubkey,
    sanitize::SanitizeError,
    serialize_utils::{read_pubkey, read_u16},
    slot_hashes::SlotHash,
    sysvar::{self, Sysvar},
};

use crate::utils::AccountDeserialize;

pub fn process_mine_coal(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // Parse args.
    let args = MineArgs::try_from_bytes(data)?;

    // Load accounts.
    let (required_accounts, optional_accounts) = accounts.split_at(6);
    let [signer, bus_info, config_info, proof_info, instructions_sysvar, slot_hashes_sysvar] = required_accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    load_signer(signer)?;
    load_any_coal_bus(bus_info, true)?;
    load_coal_config(config_info, false)?;
    load_coal_proof_with_miner(proof_info, signer.key, true)?;
    load_sysvar(instructions_sysvar, sysvar::instructions::id())?;
    load_sysvar(slot_hashes_sysvar, sysvar::slot_hashes::id())?;

    // Authenticate the proof account.
    //
    // Only one proof account can be used for any given transaction. All `mine` instructions
    // in the transaction must use the same proof account.
    authenticate_coal_proof(&instructions_sysvar.data.borrow(), proof_info.key)?;

    // Validate epoch is active.
    let config_data = config_info.data.borrow();
    let config = Config::try_from_bytes(&config_data)?;
    let clock = Clock::get().or(Err(ProgramError::InvalidAccountData))?;
    if config
        .last_reset_at
        .saturating_add(COAL_EPOCH_DURATION)
        .le(&clock.unix_timestamp)
    {
        println!("Needs reset");
        return Err(CoalError::NeedsReset.into());
    }

    // Validate the hash digest.
    //
    // Here we use drillx_2 to validate the provided solution is a valid hash of the challenge.
    // If invalid, we return an error.
    let mut proof_data = proof_info.data.borrow_mut();
    let proof = Proof::try_from_bytes_mut(&mut proof_data)?;
    let solution = Solution::new(args.digest, args.nonce);
    if !solution.is_valid(&proof.challenge) {
        return Err(CoalError::HashInvalid.into());
    }

    // Reject spam transactions.
    //
    // If a miner attempts to submit solutions too frequently, we reject with an error. In general,
    // miners are limited to 1 hash per epoch on average.
    let t: i64 = clock.unix_timestamp;
    let t_target = proof.last_hash_at.saturating_add(ONE_MINUTE);
    let t_spam = t_target.saturating_sub(TOLERANCE);
    if t.lt(&t_spam) {
        return Err(CoalError::Spam.into());
    }

    // Validate the hash satisfies the minimum difficulty.
    //
    // We use drillx_2 to get the difficulty (leading zeros) of the hash. If the hash does not have the
    // minimum required difficulty, we reject it with an error.
    let hash = solution.to_hash();
    let difficulty = hash.difficulty();
    if difficulty.lt(&(config.min_difficulty as u32)) {
        return Err(CoalError::HashTooEasy.into());
    }

    // Normalize the difficulty and calculate the reward amount.
    //
    // The reward doubles for every bit of difficulty (leading zeros) on the hash. We use the normalized
    // difficulty so the minimum accepted difficulty pays out at the base reward rate.
    let normalized_difficulty = difficulty
        .checked_sub(config.min_difficulty as u32)
        .unwrap();
    let mut reward = config
        .base_reward_rate
        .checked_mul(2u64.checked_pow(normalized_difficulty).unwrap())
        .unwrap();


    // Apply staking multiplier.
    //
    // If user has greater than or equal to the max stake on the network, they receive 2x multiplier.
    // Any stake less than this will receives between 1x and 2x multipler. The multipler is only active
    // if the miner's last stake deposit was more than one minute ago to protect against flash loan attacks.
    let mut bus_data = bus_info.data.borrow_mut();
    let bus = Bus::try_from_bytes_mut(&mut bus_data)?;
    if proof.balance.gt(&0) && proof.last_stake_at.saturating_add(ONE_MINUTE).lt(&t) {
        // Calculate staking reward.
        if config.top_balance.gt(&0) {
            let staking_reward = (reward as u128)
                .checked_mul(proof.balance.min(config.top_balance) as u128)
                .unwrap()
                .checked_div(config.top_balance as u128)
                .unwrap() as u64;
            reward = reward.checked_add(staking_reward.checked_mul(12).unwrap()).unwrap();
        }

        // Update bus stake tracker.
        if proof.balance.gt(&bus.top_balance) {
            bus.top_balance = proof.balance;
        }
    }

    // Apply liveness penalty.
    //
    // The liveness penalty exists to ensure there is no "invisible" hashpower on the network. It
    // should not be possible to spend ~1 hour on a given challenge and submit a hash with a large
    // difficulty value to earn an outsized reward.
    //
    // The penalty works by halving the reward amount for every minute late the solution has been submitted.
    // This ultimately drives the reward to zero given enough time (10-20 minutes).
    let t_liveness = t_target.saturating_add(TOLERANCE);
    if t.gt(&t_liveness) {
        // Halve the reward for every minute late.
        let tardiness = t.saturating_sub(t_target) as u64;
        let halvings = tardiness.saturating_div(ONE_MINUTE as u64);
        if halvings.gt(&0) {
            reward = reward.saturating_div(2u64.saturating_pow(halvings as u32));
        }

        // Linear decay with remainder seconds.
        let remainder_secs = tardiness.saturating_sub(halvings.saturating_mul(ONE_MINUTE as u64));
        if remainder_secs.gt(&0) && reward.gt(&0) {
            let penalty = reward
                .saturating_div(2)
                .saturating_mul(remainder_secs)
                .saturating_div(ONE_MINUTE as u64);
            reward = reward.saturating_sub(penalty);
        }
    }

    // Apply multipliers.
    let mut tool_reward: u64 = 0;
    let mut stake_reward: u64 = 0;

    if optional_accounts.len().ge(&1) {
        let mut shift: usize = 0;

        if !optional_accounts[0].data_is_empty() && is_tool(&optional_accounts[0]) {
            shift = 1;
            let tool_info = &optional_accounts[0];
            // Apply tool multiplier.
            //
            // Durability is decremented for the amount added.
            load_tool(&tool_info, signer.key, true)?;
    
            let mut tool_data = tool_info.data.borrow_mut();
            let tool = Tool::try_from_bytes_mut(&mut tool_data)?;

            if tool.durability.gt(&0) {
                // Calculate the additional reward.
                let max_additional_reward = bus.rewards.saturating_sub(reward);
                let tool_multiplier = tool.multiplier.max(BASE_TOOL_MULTIPLIER).min(MAX_TOOL_MULTIPLIER);
                let additional_reward = (reward as u128)
                    .checked_mul(tool_multiplier as u128)
                    .unwrap()
                    .checked_div(100)
                    .unwrap() as u64;
                tool_reward = additional_reward.min(tool.durability);
                msg!("tool_reward: {}", tool_reward as f64 / ONE_COAL as f64);
                reward = reward.checked_add(tool_reward).unwrap();
            
                // Durability is decremented for the amount added.
                // Only subtract the actual remaining rewards from durability.
                let actual_additional_reward = tool_reward.min(max_additional_reward);
                tool.durability = tool.durability.saturating_sub(actual_additional_reward).max(0);
            }
        }

        if optional_accounts.len().ge(&(shift + 2)) {
            let guild_config_info =  &optional_accounts[shift];
            let guild_member_info = &optional_accounts[shift + 1];
            
            let (total_stake, total_multiplier) = load_guild_config(guild_config_info)?;

            if optional_accounts.len().eq(&(shift + 3)) {
                let guild_info = &optional_accounts[shift + 2];
                let guild_stake = load_guild_with_member(guild_info, guild_member_info, signer.key)?;
                stake_reward = calculate_stake_multiplier(reward, guild_stake, total_stake, total_multiplier);
                msg!("base reward: {}", reward as f64 / ONE_COAL as f64);
                msg!("guild stake_reward: {}", stake_reward as f64 / ONE_COAL as f64);
                reward = reward.checked_add(stake_reward).unwrap();
            } else {
                let member_stake = load_member(guild_member_info, signer.key)?;
                stake_reward = calculate_stake_multiplier(reward, member_stake, total_stake, total_multiplier);
                msg!("base reward: {}", reward as f64 / ONE_COAL as f64);
                msg!("member stake_reward: {}", stake_reward as f64 / ONE_COAL as f64);
                reward = reward.checked_add(stake_reward).unwrap();
            }
        }
    }

    // Limit payout amount to whatever is left in the bus and the target per minute.
    let reward_actual = reward.min(bus.rewards).min(TARGET_COAL_REWARDS);

    // Update balances.
    //
    // We track the theoretical rewards that would have been paid out ignoring the bus limit, so the
    // base reward rate will be updated to account for the real hashpower on the network.
    bus.theoretical_rewards = bus.theoretical_rewards.checked_add(reward).unwrap();
    bus.rewards = bus.rewards.checked_sub(reward_actual).unwrap();
    proof.balance = proof.balance.checked_add(reward_actual).unwrap();

    // Hash a recent slot hash into the next challenge to prevent pre-mining attacks.
    //
    // The slot hashes are unpredictable values. By seeding the next challenge with the most recent slot hash,
    // miners are forced to submit their current solution before they can begin mining for the next.
    proof.last_hash = hash.h;
    proof.challenge = hashv(&[
        hash.h.as_slice(),
        &slot_hashes_sysvar.data.borrow()[0..size_of::<SlotHash>()],
    ])
    .0;

    // Update time trackers.
    proof.last_hash_at = t.max(t_target);

    // Update lifetime stats.
    proof.total_hashes = proof.total_hashes.saturating_add(1);
    proof.total_rewards = proof.total_rewards.saturating_add(reward_actual);

    // Log the mined rewards.
    //
    // This data can be used by off-chain indexers to display mining stats.
    set_return_data(
        MineEvent {
            difficulty: difficulty as u64,
            reward: reward_actual,
            timing: t.saturating_sub(t_liveness),
            tool_reward,
            stake_reward,
        }
        .to_bytes(),
    );

    Ok(())
}

/// Authenticate the proof account.
///
/// This process is necessary to prevent sybil attacks. If a user can pack multiple hashes into a single
/// transaction, then there is a financial incentive to mine across multiple keypairs and submit as many hashes
/// as possible in the same transaction to minimize fee / hash.
///
/// This is prevented by forcing every transaction to declare upfront the proof account that will be used for mining.
/// The authentication process includes passing the 32 byte pubkey address as instruction data to a CU-optimized noop
/// program. We parse this address through transaction introspection and use it to ensure the same proof account is
/// used for every `mine` instruction in a given transaction.
fn authenticate_coal_proof(data: &[u8], proof_address: &Pubkey) -> ProgramResult {
    if let Ok(Some(auth_address)) = parse_coal_auth_address(data) {
        if proof_address.ne(&auth_address) {
            return Err(CoalError::AuthFailed.into());
        }
    } else {
        return Err(CoalError::AuthFailed.into());
    }
    Ok(())
}

/// Use transaction introspection to parse the authenticated pubkey.
fn parse_coal_auth_address(data: &[u8]) -> Result<Option<Pubkey>, SanitizeError> {
    let mut curr = 0;
    let num_instructions = read_u16(&mut curr, data)?;
    let pc = curr;

    let mut noop_count = 0;

    for i in 0..num_instructions as usize {
        curr = pc + i * 2;
        curr = read_u16(&mut curr, data)? as usize;

        let num_accounts = read_u16(&mut curr, data)? as usize;
        curr += num_accounts * 33;

        let program_id = read_pubkey(&mut curr, data)?;

        if program_id.eq(&NOOP_PROGRAM_ID) {
            noop_count += 1;
            
            if noop_count == 2 {
                curr += 2;
                let address = read_pubkey(&mut curr, data)?;
                return Ok(Some(address));
            }
        }
    }

    Ok(None)
}

fn calculate_stake_multiplier(base_reward: u64, stake: u64, total_stake: u64, multiplier: u64) -> u64 {
    (base_reward as u128)
        .checked_mul(multiplier as u128)
        .unwrap()
        .checked_mul(stake as u128)
        .unwrap()
        .checked_div(total_stake as u128)
        .unwrap() as u64
}