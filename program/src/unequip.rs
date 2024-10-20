use coal_api::{consts::*, instruction::UnequipArgs, loaders::*};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError, system_program
};
use mpl_core::{instructions::{TransferV1CpiBuilder, UpdatePluginV1CpiBuilder}, types::{Attribute, Attributes, Plugin}, Asset};

/// Closes the tool account and updates the durability attribute.
pub fn process_unequip_tool<'a, 'info>(accounts: &'a [AccountInfo<'info>], data: &[u8]) -> ProgramResult {
    // Parse args.
    let args = UnequipArgs::try_from_bytes(data)?;

    // Load accounts.
    let [signer, miner_info, payer_info, asset_info, collection_info, tool_info, plugin_update_authority, mpl_core_program, system_program] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

	load_signer(signer)?;
	load_program(mpl_core_program, mpl_core::ID)?;
    load_program(system_program, system_program::id())?;

	// Update durability attribute
    let durability = load_any_tool_with_asset(tool_info, miner_info.key, asset_info.key, true)?;
	let mut updated_attributes = vec![
		Attribute {
			key: "durability".to_string(),
			value: amount_u64_to_f64(durability).to_string()
		},
	];

	// Update other attributes
	let asset = Asset::from_bytes(&asset_info.data.borrow()).unwrap();
	let attributes_plugin = asset.plugin_list.attributes.unwrap();
	let resource = attributes_plugin.attributes.attribute_list.iter().find(|attr| attr.key == "resource").unwrap().value.clone();

	attributes_plugin.attributes.attribute_list.iter().for_each(|attr| {
		if attr.key != "durability" {
			updated_attributes.push(Attribute {
				key: attr.key.clone(),
				value: attr.value.clone(),
			});
		}
	});

	let plugin_authority_seeds = &[b"update_authority".as_ref(), &[args.plugin_authority_bump]];
	// Update attributes CPI
	UpdatePluginV1CpiBuilder::new(mpl_core_program)
		.asset(asset_info)
		.collection(Some(collection_info))
		.payer(signer)
		.authority(Some(plugin_update_authority))
		.plugin(Plugin::Attributes(Attributes {
			attribute_list: updated_attributes
		}))
		.system_program(system_program)
		.invoke_signed(&[plugin_authority_seeds])?;

    // Realloc data to zero.
    tool_info.realloc(0, true)?;
    // Send remaining lamports to signer.
    **signer.lamports.borrow_mut() += tool_info.lamports();
    **tool_info.lamports.borrow_mut() = 0;


	// Transfer tool to signer
	let seed = match resource.as_str() {
		"coal" => COAL_MAIN_HAND_TOOL,
		"wood" => WOOD_MAIN_HAND_TOOL,
		_ => return Err(ProgramError::InvalidAccountData),
	};
	let signer_seeds = &[seed, signer.key.as_ref(), &[args.bump]];
	
	TransferV1CpiBuilder::new(mpl_core_program)
	  .asset(asset_info)
	  .collection(Some(collection_info))
	  .payer(payer_info)
	  .authority(Some(tool_info))
	  .new_owner(signer)
	  .system_program(Some(system_program))
	  .invoke_signed(&[signer_seeds])?;

	Ok(())
}