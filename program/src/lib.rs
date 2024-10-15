mod claim;
mod close;
// mod init_coal;
// mod init_wood;
mod init_chromium;
mod mine;
mod open_coal;
mod open_wood;
mod reset;
mod stake;
mod update;
mod equip;
mod unequip;
mod reprocess;
use claim::*;
use close::*;
// use init_coal::*;
// use init_wood::*;
use init_chromium::*;
use mine::*;
use open_coal::*;
use open_wood::*;
use reset::*;
use stake::*;
use update::*;
use equip::*;
use unequip::*;
use reprocess::finalize_reprocess::*;
use reprocess::init_reprocess::*;
use coal_api::instruction::*;
use solana_program::{
    self, account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

pub(crate) use coal_utils as utils;

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    if program_id.ne(&coal_api::id()) {
        println!("Program ID mismatch");
        return Err(ProgramError::IncorrectProgramId);
    }

    let (tag, data) = data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    println!("Validated instruction data");
    match CoalInstruction::try_from(*tag).or(Err(ProgramError::InvalidInstructionData))? {
        CoalInstruction::Claim => process_claim(accounts, data)?,
        CoalInstruction::Close => process_close(accounts, data)?,
        CoalInstruction::Mine => process_mine(accounts, data)?,
        CoalInstruction::OpenCoal => process_open_coal(accounts, data)?,
        CoalInstruction::OpenWood => process_open_wood(accounts, data)?,
        CoalInstruction::Reset => process_reset(accounts, data)?,
        CoalInstruction::Stake => process_stake(accounts, data)?,
        CoalInstruction::Update => process_update(accounts, data)?,
        // CoalInstruction::InitCoal => process_init_coal(accounts, data)?,
        // CoalInstruction::InitWood => process_init_wood(accounts, data)?,
        CoalInstruction::InitChromium => process_init_chromium(accounts, data)?,
        CoalInstruction::Equip => process_equip_tool(accounts, data)?,
        CoalInstruction::Unequip => process_unequip_tool(accounts, data)?,
        CoalInstruction::InitReprocess => process_initialize_reprocess(accounts, data)?,
        CoalInstruction::FinalizeReprocess => process_finalize_reprocess(accounts, data)?,
    }

    Ok(())
}
