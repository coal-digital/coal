use std::mem::size_of;

use coal_api::{consts::*, instruction::EquipArgs, loaders::*, state::Tool};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError, system_program
};
use mpl_core::instructions::TransferV1CpiBuilder;

use crate::utils::{create_pda, AccountDeserialize, Discriminator};

/// Creates a new tool account and transfers the asset to the tool.
pub fn process_equip_tool<'a, 'info>(accounts: &'a [AccountInfo<'info>], data: &[u8]) -> ProgramResult {
    // Parse args.
    let args = EquipArgs::try_from_bytes(data)?;

    // Load accounts.
    let [signer, miner_info, payer_info, asset_info, collection_info, tool_info, mpl_core, system_program] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    load_signer(signer)?;
    load_any(miner_info, false)?;
    load_signer(payer_info)?;
    load_uninitialized_pda(
        tool_info,
        &[COAL_MAIN_HAND_TOOL, signer.key.as_ref()],
        args.bump,
        &coal_api::id(),
    )?;
	load_program(mpl_core, mpl_core::ID)?;
    load_program(system_program, system_program::id())?;

    // Initialize proof.
    create_pda(
        tool_info,
        &coal_api::id(),
        8 + size_of::<Tool>(),
        &[COAL_MAIN_HAND_TOOL, signer.key.as_ref(), &[args.bump]],
        system_program,
        payer_info,
    )?;
	
	TransferV1CpiBuilder::new(mpl_core)
        .asset(asset_info)
        .collection(Some(collection_info))
        .payer(payer_info)
        .authority(Some(signer))
        .new_owner(tool_info)
        .system_program(Some(system_program))
        .invoke()?;

	let (durability, multiplier) = load_asset(asset_info)?;
	msg!("durability: {}", durability);
	msg!("multiplier: {}", multiplier);
	
    let mut tool_data = tool_info.data.borrow_mut();
    tool_data[0] = Tool::discriminator() as u8;
    let tool = Tool::try_from_bytes_mut(&mut tool_data)?;
	tool.authority = *signer.key;
	tool.miner = *miner_info.key;
	tool.asset = *asset_info.key;
	tool.durability = amount_f64_to_u64(durability);
	tool.multiplier = multiplier;

    msg!("tool durability: {}", tool.durability);
    msg!("tool multiplier: {}", tool.multiplier);

	Ok(())
}
