use cosmwasm_std::{to_json_binary, Addr, Coin, Deps, QueryRequest, StdResult, Uint64, WasmQuery};

use white_whale_std::fee_collector::{Config, ContractType, FactoryType, FeesFor};
use white_whale_std::pool_network;
use white_whale_std::pool_network::asset::{Asset, AssetInfo};
use white_whale_std::pool_network::factory::PairsResponse;
use white_whale_std::pool_network::pair::ProtocolFeesResponse as ProtocolPairFeesResponse;
use white_whale_std::vault_network::vault::ProtocolFeesResponse as ProtocolVaultFeesResponse;
use white_whale_std::vault_network::vault_factory::VaultsResponse;

use crate::state::{CONFIG, TAKE_RATE_HISTORY};

/// Queries the [Config], which contains the owner address
pub fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

/// Queries the take rate for the given epoch id
pub fn query_take_rate_history(deps: Deps, epoch_id: Uint64) -> StdResult<Coin> {
    let take_rate = TAKE_RATE_HISTORY.load(deps.storage, epoch_id.u64())?;
    Ok(take_rate)
}

/// Queries the fees in [Asset] for contracts or Factories defined by [FeesFor]
pub fn query_fees(deps: Deps, query_fees_for: FeesFor, all_time: bool) -> StdResult<Vec<Asset>> {
    let mut fees: Vec<Asset> = Vec::new();

    match query_fees_for {
        FeesFor::Contracts { mut contracts } => {
            contracts.dedup_by(|a, b| a.address == b.address);

            for contract in contracts {
                match contract.contract_type {
                    ContractType::Pool {} => {
                        let mut pair_fee =
                            query_fees_for_pair(&deps, contract.address.clone(), all_time)?;

                        fees.append(&mut pair_fee);
                    }
                    ContractType::Vault {} => {
                        let vault_fee =
                            query_fees_for_vault(&deps, contract.address.clone(), all_time)?;

                        fees.push(vault_fee);
                    }
                }
            }
        }
        FeesFor::Factory {
            factory_addr,
            factory_type,
        } => {
            let factory = deps.api.addr_validate(factory_addr.as_str())?;
            let mut assets = query_fees_for_factory(&deps, &factory, factory_type, all_time)?;

            fees.append(&mut assets);
        }
    }

    // accumulate fees, as the asset fees coming from different pairs, i.e. pair_fees,
    // would be duplicated in the fees vector
    fees = fees
        .into_iter()
        .fold(Vec::<Asset>::new(), |mut acc, asset| {
            let accumulated_asset = acc.iter_mut().find(|a| a.info == asset.info);
            match accumulated_asset {
                Some(accumulated_asset) => accumulated_asset.amount += asset.amount,
                None => acc.push(asset),
            }

            acc
        });

    Ok(fees)
}

/// Queries the fees for a given vault
fn query_fees_for_vault(deps: &Deps, vault: String, all_time: bool) -> StdResult<Asset> {
    let fees = deps
        .querier
        .query::<ProtocolVaultFeesResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: vault,
            msg: to_json_binary(
                &white_whale_std::vault_network::vault::QueryMsg::ProtocolFees { all_time },
            )?,
        }))?
        .fees;

    Ok(fees)
}

/// Queries the fees for a given pair
fn query_fees_for_pair(deps: &Deps, pair: String, all_time: bool) -> StdResult<Vec<Asset>> {
    let fees = deps
        .querier
        .query::<ProtocolPairFeesResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: pair,
            msg: to_json_binary(&pool_network::pair::QueryMsg::ProtocolFees {
                all_time: Some(all_time),
                asset_id: None,
            })?,
        }))?
        .fees;

    Ok(fees)
}

/// Queries the fees collected by the children of the given factory
fn query_fees_for_factory(
    deps: &Deps,
    factory: &Addr,
    factory_type: FactoryType,
    all_time: bool,
) -> StdResult<Vec<Asset>> {
    let mut fees: Vec<Asset> = Vec::new();

    match factory_type {
        FactoryType::Vault { start_after, limit } => {
            let response: VaultsResponse =
                deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: factory.to_string(),
                    msg: to_json_binary(
                        &white_whale_std::vault_network::vault_factory::QueryMsg::Vaults {
                            start_after,
                            limit,
                        },
                    )?,
                }))?;

            for vault_info in response.vaults {
                let vault_fee = query_fees_for_vault(deps, vault_info.vault, all_time)?;
                fees.push(vault_fee);
            }
        }
        FactoryType::Pool { start_after, limit } => {
            let response: PairsResponse =
                deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: factory.to_string(),
                    msg: to_json_binary(&pool_network::factory::QueryMsg::Pairs {
                        start_after,
                        limit,
                    })?,
                }))?;

            for pair in response.pairs {
                let mut pair_fees = query_fees_for_pair(deps, pair.contract_addr, all_time)?;
                fees.append(&mut pair_fees);
            }
        }
    }

    Ok(fees)
}

/// Queries the fee collector to get the distribution asset
pub(crate) fn query_distribution_asset(deps: Deps) -> StdResult<AssetInfo> {
    let config: Config = CONFIG.load(deps.storage)?;

    let fee_distributor_config: white_whale_std::fee_distributor::Config =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.fee_distributor.to_string(),
            msg: to_json_binary(&white_whale_std::fee_distributor::QueryMsg::Config {})?,
        }))?;

    Ok(fee_distributor_config.distribution_asset)
}
