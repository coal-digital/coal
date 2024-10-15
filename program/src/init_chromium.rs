use coal_api::{
    consts::*,
    instruction::*,
    loaders::*,
};
use coal_utils::spl::create_ata;
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    program_error::ProgramError,
    program_pack::Pack,
    system_program, {self, sysvar},
};
use spl_token::state::Mint;

use crate::utils::create_pda;

/// Initialize sets up the ORE program to begin mining.
pub fn process_init_chromium<'a, 'info>(
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
) -> ProgramResult {
    // Parse args.
    let args = InitChromiumArgs::try_from_bytes(data)?;

    // Load accounts.
    let [signer, mint_info, metadata_info, treasury_info, treasury_tokens_info, system_program, token_program, associated_token_program, metadata_program, rent_sysvar] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    load_signer(signer)?;
    load_uninitialized_pda(
        metadata_info,
        &[
            METADATA,
            mpl_token_metadata::ID.as_ref(),
            CHROMIUM_MINT_ADDRESS.as_ref(),
        ],
        args.metadata_bump,
        &mpl_token_metadata::ID,
    )?;
    load_uninitialized_pda(
        mint_info,
        &[CHROMIUM_MINT, MINT_NOISE.as_slice()],
        args.mint_bump,
        &coal_api::id(),
    )?;
    load_system_account(treasury_tokens_info, true)?;
    load_program(system_program, system_program::id())?;
    load_program(token_program, spl_token::id())?;
    load_program(associated_token_program, spl_associated_token_account::id())?;
    load_program(metadata_program, mpl_token_metadata::ID)?;
    load_sysvar(rent_sysvar, sysvar::rent::id())?;

    // Check signer.
    if signer.key.ne(&INITIALIZER_ADDRESS) {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Initialize mint.
    create_pda(
        mint_info,
        &spl_token::id(),
        Mint::LEN,
        &[CHROMIUM_MINT, MINT_NOISE.as_slice(), &[args.mint_bump]],
        system_program,
        signer,
    )?;
    solana_program::program::invoke_signed(
        &spl_token::instruction::initialize_mint(
            &spl_token::id(),
            mint_info.key,
            treasury_info.key,
            None,
            TOKEN_DECIMALS,
        )?,
        &[
            token_program.clone(),
            mint_info.clone(),
            treasury_info.clone(),
            rent_sysvar.clone(),
        ],
        &[&[CHROMIUM_MINT, MINT_NOISE.as_slice(), &[args.mint_bump]]],
    )?;

    // Initialize mint metadata.
    mpl_token_metadata::instructions::CreateMetadataAccountV3Cpi {
        __program: metadata_program,
        metadata: metadata_info,
        mint: mint_info,
        mint_authority: treasury_info,
        payer: signer,
        update_authority: (signer, true),
        system_program,
        rent: Some(rent_sysvar),
        __args: mpl_token_metadata::instructions::CreateMetadataAccountV3InstructionArgs {
            data: mpl_token_metadata::types::DataV2 {
                name: CHROMIUM_METADATA_NAME.to_string(),
                symbol: CHROMIUM_METADATA_SYMBOL.to_string(),
                uri: CHROMIUM_METADATA_URI.to_string(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            is_mutable: true,
            collection_details: None,
        },
    }
    .invoke_signed(&[&[TREASURY, &[args.treasury_bump]]])?;

    // Initialize treasury token account.
    create_ata(
        signer,
        treasury_info,
        treasury_tokens_info,
        mint_info,
        system_program,
        token_program,
        associated_token_program,
    )?;

    Ok(())
}
