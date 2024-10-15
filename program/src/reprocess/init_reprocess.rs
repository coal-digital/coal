use std::mem::size_of;

use coal_api::{consts::*, instruction::ReprocessArgs, loaders::*, state::Reprocessor};
use solana_program::{
    account_info::AccountInfo,
    clock::Clock,
    entrypoint::ProgramResult,
    keccak::hashv,
    native_token::LAMPORTS_PER_SOL,
    program::invoke,
    program_error::ProgramError,
    slot_hashes::SlotHash, 
    system_instruction::transfer, 
    sysvar::{self, Sysvar}
};

use crate::utils::{create_pda, AccountDeserialize, Discriminator};


pub fn process_initialize_reprocess(accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    // Parse args.
    let args = ReprocessArgs::try_from_bytes(data)?;

    // Load accounts.
    let [signer, treasury_info, reprocessor_info, slot_hashes_sysvar, system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    load_signer(signer)?;
    load_treasury(treasury_info, true)?;
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
    reprocessor.slot = slot + 20;
    reprocessor.hash = hashv(&[
        &slot_hashes_sysvar.data.borrow()[0..size_of::<SlotHash>()],
    ])
    .0;
    

    // Transfer fee of 0.005 SOL to treasury
    // This is to discourage abuse
    let fee: u64 = LAMPORTS_PER_SOL / 200;
    let transfer_ix = transfer(
        signer.key,
        treasury_info.key,
        fee,
    );
    invoke(
        &transfer_ix,
        &[signer.clone(), treasury_info.clone(), system_program.clone()],
    )?;

    Ok(())
}