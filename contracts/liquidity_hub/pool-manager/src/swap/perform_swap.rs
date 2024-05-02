use cosmwasm_std::{Coin, Decimal, DepsMut, Uint128};

use white_whale_std::pool_manager::PoolInfo;
use white_whale_std::pool_network::swap::assert_max_spread;

use crate::{
    helpers,
    state::{get_pool_by_identifier, POOLS},
    ContractError,
};

#[derive(Debug)]
pub struct SwapResult {
    /// The asset that should be returned to the user from the swap.
    pub return_asset: Coin,
    /// The burn fee of `return_asset` associated with this swap transaction.
    pub burn_fee_asset: Coin,
    /// The protocol fee of `return_asset` associated with this swap transaction.
    pub protocol_fee_asset: Coin,
    /// The swap fee of `return_asset` associated with this swap transaction.
    pub swap_fee_asset: Coin,
    /// The osmosis fee of `return_asset` associated with this swap transaction.
    #[cfg(feature = "osmosis")]
    pub osmosis_fee_asset: Coin,
    /// The pool that was traded.
    pub pool_info: PoolInfo,
    /// The amount of spread that occurred during the swap from the original exchange rate.
    pub spread_amount: Uint128,
}

/// Attempts to perform a swap from `offer_asset` to the relevant opposing
/// asset in the pool identified by `pool_identifier`.
///
/// Assumes that `offer_asset` is a **native token**.
///
/// The resulting [`SwapResult`] has actions that should be taken, as the swap has been performed.
/// In other words, the caller of the `perform_swap` function _should_ make use
/// of each field in [`SwapResult`] (besides fields like `spread_amount`).
pub fn perform_swap(
    deps: DepsMut,
    offer_asset: Coin,
    pool_identifier: String,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
) -> Result<SwapResult, ContractError> {
    let mut pool_info = get_pool_by_identifier(&deps.as_ref(), &pool_identifier)?;
    let pools = &pool_info.assets;

    // compute the offer and ask pool
    let offer_pool: Coin;
    let ask_pool: Coin;
    let offer_decimal: u8;
    let ask_decimal: u8;
    let decimals = &pool_info.asset_decimals;

    // calculate the swap
    // first, set relevant variables
    if offer_asset.denom == pools[0].denom {
        offer_pool = pools[0].clone();
        ask_pool = pools[1].clone();
        offer_decimal = decimals[0];
        ask_decimal = decimals[1];
    } else if offer_asset.denom == pools[1].denom {
        offer_pool = pools[1].clone();
        ask_pool = pools[0].clone();

        offer_decimal = decimals[1];
        ask_decimal = decimals[0];
    } else {
        return Err(ContractError::AssetMismatch);
    }

    let offer_amount = offer_asset.amount;
    let pool_fees = pool_info.pool_fees.clone();

    let swap_computation = helpers::compute_swap(
        offer_pool.amount,
        ask_pool.amount,
        offer_amount,
        pool_fees,
        &pool_info.pool_type,
        offer_decimal,
        ask_decimal,
    )?;

    let return_asset = Coin {
        denom: ask_pool.denom.clone(),
        amount: swap_computation.return_amount,
    };

    // Assert spread and other operations
    // check max spread limit if exist
    assert_max_spread(
        belief_price,
        max_spread,
        offer_asset.amount,
        return_asset.amount,
        swap_computation.spread_amount,
    )?;

    // State changes to the pools balances
    // Deduct the return amount from the pool and add the offer amount to the pool
    if offer_asset.denom == pools[0].denom {
        pool_info.assets[0].amount += offer_amount;
        pool_info.assets[1].amount -= swap_computation.return_amount;
        pool_info.assets[1].amount -= swap_computation.protocol_fee_amount;
        pool_info.assets[1].amount -= swap_computation.burn_fee_amount;
    } else {
        pool_info.assets[1].amount += offer_amount;
        pool_info.assets[0].amount -= swap_computation.return_amount;
        pool_info.assets[0].amount -= swap_computation.protocol_fee_amount;
        pool_info.assets[0].amount -= swap_computation.burn_fee_amount;
    }

    POOLS.save(deps.storage, &pool_identifier, &pool_info)?;

    let burn_fee_asset = Coin {
        denom: ask_pool.denom.clone(),
        amount: swap_computation.burn_fee_amount,
    };
    let protocol_fee_asset = Coin {
        denom: ask_pool.denom.clone(),
        amount: swap_computation.protocol_fee_amount,
    };

    #[allow(clippy::redundant_clone)]
    let swap_fee_asset = Coin {
        denom: ask_pool.denom.clone(),
        amount: swap_computation.swap_fee_amount,
    };

    #[cfg(not(feature = "osmosis"))]
    {
        Ok(SwapResult {
            return_asset,
            swap_fee_asset,
            burn_fee_asset,
            protocol_fee_asset,
            pool_info,
            spread_amount: swap_computation.spread_amount,
        })
    }

    #[cfg(feature = "osmosis")]
    {
        let osmosis_fee_asset = Coin {
            denom: ask_pool.denom,
            amount: swap_computation.swap_fee_amount,
        };

        Ok(SwapResult {
            return_asset,
            swap_fee_asset,
            burn_fee_asset,
            protocol_fee_asset,
            osmosis_fee_asset,
            pool_info,
            spread_amount: swap_computation.spread_amount,
        })
    }
}
