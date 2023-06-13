use cosmwasm_std::{Decimal, DepsMut, MessageInfo};
use white_whale::fee_distributor::ClaimableEpochsResponse;
use white_whale::pool_network::asset::{Asset, AssetInfo};

use crate::error::ContractError;
use crate::state::CONFIG;

/// Validates that the growth rate is between 0 and 1.
pub fn validate_growth_rate(growth_rate: Decimal) -> Result<(), ContractError> {
    if growth_rate > Decimal::percent(100) {
        return Err(ContractError::InvalidGrowthRate {});
    }
    Ok(())
}

/// Validates that the asset sent on the message matches the asset provided and is whitelisted for bonding.
pub fn validate_funds(
    deps: &DepsMut,
    info: &MessageInfo,
    asset: &Asset,
    denom: String,
) -> Result<(), ContractError> {
    let bonding_assets = CONFIG.load(deps.storage)?.bonding_assets;

    if info.funds.len() != 1
        || info.funds[0].amount.is_zero()
        || info.funds[0].amount != asset.amount
        || info.funds[0].denom != denom
        || !bonding_assets.iter().any(|asset_info| {
            let d = match asset_info {
                AssetInfo::NativeToken { denom } => denom.clone(),
                AssetInfo::Token { .. } => String::new(),
            };
            d == denom
        })
    {
        return Err(ContractError::AssetMismatch {});
    }

    Ok(())
}

// if user has unclaimed rewards, fail with an exception prompting them to claim
pub fn validate_claimed(deps: &DepsMut, info: &MessageInfo) -> Result<(), ContractError> {
    // Query fee distributor
    // if user has unclaimed rewards, fail with an exception prompting them to claim
    let config = CONFIG.load(deps.storage)?;
    let fee_distributor = config.fee_distributor_addr;

    // Do a smart query for Claimable
    let claimable_rewards: ClaimableEpochsResponse = deps
        .querier
        .query_wasm_smart(
            fee_distributor,
            &white_whale::fee_distributor::QueryMsg::Claimable {
                address: info.sender.to_string(),
            },
        )
        .unwrap();

    // If epochs is greater than none
    if !claimable_rewards.epochs.is_empty() {
        return Err(ContractError::UnclaimedRewards {});
    }

    Ok(())
}
