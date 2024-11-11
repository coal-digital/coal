use solana_program::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
};
use coal_guilds_api::state::{Guild, Member, Config as GuildConfig};
use steel::AccountDeserialize;

pub fn load_guild_with_member<'a, 'info>(guild_info: &'a AccountInfo<'info>, member_info: &'a AccountInfo<'info>, authority: &Pubkey) -> Result<u64, ProgramError> {
    if guild_info.owner.ne(&coal_guilds_api::id()) {
        return Err(ProgramError::InvalidAccountOwner);
    }

    if member_info.owner.ne(&coal_guilds_api::id()) {
        return Err(ProgramError::InvalidAccountOwner);
    }

    let guild_data = &guild_info.data.borrow();
    let member_data = &member_info.data.borrow();

    let guild = Guild::try_from_bytes(guild_data)?;
    let member = Member::try_from_bytes(member_data)?;

    if member.authority.ne(authority) {
        return Err(ProgramError::InvalidAccountOwner);
    }

    if member.is_active.ne(&1) {
        return Err(ProgramError::InvalidAccountData);
    }

    if member.guild.ne(&guild_info.key) {
        return Err(ProgramError::InvalidAccountOwner);
    }

    Ok(guild.total_stake)
}

pub fn load_member<'a, 'info>(member_info: &'a AccountInfo<'info>, authority: &Pubkey) -> Result<u64, ProgramError> {
    if member_info.owner.ne(&coal_guilds_api::id()) {
        return Err(ProgramError::InvalidAccountOwner);
    }

    let member_data = &member_info.data.borrow();
    let member = Member::try_from_bytes(member_data)?;

    if member.authority.ne(authority) {
        return Err(ProgramError::InvalidAccountData);
    }

    if member.is_active.ne(&1) {
        return Err(ProgramError::InvalidAccountData);
    }

    Ok(member.total_stake)
}

pub fn load_guild_config<'a, 'info>(guild_config_info: &'a AccountInfo<'info>) -> Result<(u64, u64), ProgramError> {
    if guild_config_info.owner.ne(&coal_guilds_api::id()) {
        return Err(ProgramError::InvalidAccountOwner);
    }

    let guild_config_data = &guild_config_info.data.borrow();
    let guild_config = GuildConfig::try_from_bytes(guild_config_data)?;

    Ok((guild_config.total_stake, guild_config.total_multiplier))
}
